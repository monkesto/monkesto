mod account;
mod auth;
mod authority;
mod authz;
mod email;
mod entitlement;
mod event;
mod id;
mod journal;
mod monkesto_error;
pub mod name;
mod notfoundpage;
mod postcard;
mod seed;
mod theme;
mod time_provider;
mod transaction;
pub mod util;

pub mod store;

use crate::account::AccountMemoryStore;
use crate::account::AccountService;
use crate::auth::{AuthEvent, AuthInterface};
use crate::journal::JournalMemoryStore;
use crate::journal::JournalService;
use crate::transaction::TransactionMemoryStore;
use crate::transaction::TransactionService;
use axum::Router;
use axum::extract::FromRef;
use axum::http::header;
use axum::http::{Response, StatusCode};
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::routing::get;
use axum_login::tracing::{Level, Span};
use axum_login::{AuthManagerLayerBuilder, tracing};
use disintegrate::serde::json::Json;
use disintegrate_postgres::{PgEventStore, PgSnapshotter, WithPgSnapshot, decision_maker};
use dotenvy::dotenv;
use seed::seed_dev_data;
use sqlx::PgPool;
use std::env;
use std::time::Duration;
use tokio::signal;
use tower_http::services::ServeFile;
use tower_http::trace::TraceLayer;
use tower_sessions::SessionManagerLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

type MemoryJournalService = JournalService<JournalMemoryStore>;
type MemoryAccountService = AccountService<AccountMemoryStore, JournalMemoryStore>;
type MemoryTransactionService =
    TransactionService<TransactionMemoryStore, AccountMemoryStore, JournalMemoryStore>;

#[derive(Clone)]
struct AppState {
    auth_interface: AuthInterface,
    journal_service: MemoryJournalService,
    account_service: MemoryAccountService,
    transaction_service: MemoryTransactionService,
}

impl AppState {
    fn new(auth_interface: AuthInterface) -> Self {
        let journal_store = JournalMemoryStore::new();
        let account_store = AccountMemoryStore::new();
        let transaction_store = TransactionMemoryStore::new();

        let journal_service = JournalService::new(journal_store, auth_interface.clone());
        let account_service = AccountService::new(account_store, journal_service.clone());
        let transaction_service = TransactionService::new(
            transaction_store,
            account_service.clone(),
            journal_service.clone(),
        );

        Self {
            auth_interface,
            journal_service,
            account_service,
            transaction_service,
        }
    }
}

impl FromRef<AppState> for MemoryJournalService {
    fn from_ref(state: &AppState) -> Self {
        state.journal_service.clone()
    }
}

impl FromRef<AppState> for MemoryAccountService {
    fn from_ref(state: &AppState) -> Self {
        state.account_service.clone()
    }
}

impl FromRef<AppState> for MemoryTransactionService {
    fn from_ref(state: &AppState) -> Self {
        state.transaction_service.clone()
    }
}

type StateType = AppState;
type BackendType = AuthInterface;

#[tokio::main]
async fn main() {
    dotenv().ok();

    if env::var("RUST_LOG").is_err() {
        unsafe {
            // Concurrent writing of set_var is not permitted,
            // but we're in main, so that shouldn't be a problem.
            env::set_var("RUST_LOG", "INFO");
        }
    }

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::filter::LevelFilter::from_level(
            Level::DEBUG,
        ))
        .init();

    let addr = env::var("SITE_ADDR").unwrap_or("0.0.0.0:3000".to_string());

    // other stores will have to namespace themselves or use a separate database to avoid event table conflicts
    let auth_pool = PgPool::connect(
        env::var("DATABASE_URL")
            .expect("failed to fetch database url")
            .as_str(),
    )
    .await
    .expect("failed to create pgpool");

    let session_store = tower_sessions_sqlx_store::PostgresStore::new(auth_pool.clone());
    let session_layer = SessionManagerLayer::new(session_store);

    let serde = Json::<AuthEvent>::default();
    let auth_event_store = PgEventStore::try_new(auth_pool.clone(), serde)
        .await
        .expect("failed to create an auth event store");
    let snapshotter = PgSnapshotter::try_new(auth_pool.clone(), 10)
        .await
        .expect("failed to create an auth snapshotter");
    let decision_maker = decision_maker(auth_event_store.clone(), WithPgSnapshot::new(snapshotter));

    let auth_interface = AuthInterface::try_new(auth_pool.clone(), decision_maker)
        .await
        .expect("failed to create a projection pool");

    tokio::spawn(auth::event_listener(
        auth_event_store.clone(),
        auth_interface.clone(),
    ));

    let state = AppState::new(auth_interface.clone());

    seed_dev_data(&state)
        .await
        .expect("Failed to seed dev data");

    // use the service's user_store so that the data syncs
    let auth_layer = AuthManagerLayerBuilder::new(auth_interface.clone(), session_layer).build();

    let webauthn_routes =
        auth::router(auth_interface.clone()).expect("Failed to initialize WebAuthn routes");

    let journal_routes = journal::router()
        .merge(account::router())
        .merge(transaction::router());

    // the dockerfile defines this for production deployments
    let site_root = env::var("SITE_ROOT").unwrap_or_else(|_| "target/site".to_string());

    let app = Router::new()
        .route("/favicon.ico", get(serve_favicon))
        .route("/logo.svg", get(serve_logo))
        .route_service(
            "/monkesto.css",
            ServeFile::new(format!("{}/pkg/monkesto.css", site_root)),
        )
        .route("/", get(Redirect::to("/journal")))
        .merge(webauthn_routes)
        .merge(journal_routes)
        .fallback(notfoundpage::not_found_page)
        .layer(auth_layer)
        .layer(TraceLayer::new_for_http().on_response(
            |response: &Response<_>, latency: Duration, _span: &Span| {
                tracing::info!(
                    status = %response.status(),
                    latency_μs = latency.as_micros(),
                    "response"
                );
            },
        ));

    let app = app.with_state(state);

    // run our app with hyper
    println!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind the tcp address");

    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown())
        .await
        .expect("failed to serve on the address");
}

async fn serve_favicon() -> impl IntoResponse {
    const FAVICON_BYTES: &[u8] = include_bytes!("favicon.ico");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/x-icon")],
        FAVICON_BYTES,
    )
}

async fn serve_logo() -> impl IntoResponse {
    const LOGO_SVG: &str = include_str!("logo.svg");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/svg+xml")],
        LOGO_SVG,
    )
}

async fn shutdown() {
    signal::ctrl_c()
        .await
        .expect("failed to listen for an interrupt signal")
}
