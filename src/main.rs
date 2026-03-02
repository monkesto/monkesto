mod account;
mod auth;
mod authority;
mod event;
mod ident;
mod journal;
mod known_errors;
mod notfoundpage;
mod queue;
mod seed;
mod theme;
mod transaction;

use axum::Router;
use axum::extract::FromRef;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::routing::get;
use axum_login::AuthManagerLayerBuilder;
use dotenvy::dotenv;
use std::env;

use crate::account::AccountMemoryStore;
use crate::account::AccountService;
use crate::auth::MemoryUserStore;
use crate::auth::UserService;
use crate::journal::JournalMemoryStore;
use crate::journal::JournalService;
use crate::transaction::TransactionMemoryStore;
use crate::transaction::TransactionService;
use seed::seed_dev_data;
use tower_http::services::ServeFile;
use tower_sessions::SessionManagerLayer;
use tower_sessions_file_store::FileSessionStorage;

type MemoryJournalService = JournalService<JournalMemoryStore, MemoryUserStore>;
type MemoryUserService = UserService<MemoryUserStore>;
type MemoryAccountService = AccountService<AccountMemoryStore, JournalMemoryStore, MemoryUserStore>;
type MemoryTransactionService = TransactionService<
    TransactionMemoryStore,
    AccountMemoryStore,
    JournalMemoryStore,
    MemoryUserStore,
>;

#[derive(Clone)]
struct AppState {
    user_service: MemoryUserService,
    journal_service: MemoryJournalService,
    account_service: MemoryAccountService,
    transaction_service: MemoryTransactionService,
}

impl AppState {
    fn new() -> Self {
        let user_store = MemoryUserStore::new();
        let journal_store = JournalMemoryStore::new();
        let account_store = AccountMemoryStore::new();
        let transaction_store = TransactionMemoryStore::new();

        let user_service = UserService::new(user_store.clone());
        let journal_service = JournalService::new(journal_store, user_store);
        let account_service = AccountService::new(account_store, journal_service.clone());
        let transaction_service = TransactionService::new(
            transaction_store,
            account_service.clone(),
            journal_service.clone(),
        );

        Self {
            user_service,
            journal_service,
            account_service,
            transaction_service,
        }
    }
}

impl FromRef<AppState> for MemoryUserService {
    fn from_ref(state: &AppState) -> Self {
        state.user_service.clone()
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
type BackendType = MemoryUserStore;

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
    tracing_subscriber::fmt::init();

    let addr = env::var("SITE_ADDR").unwrap_or("0.0.0.0:3000".to_string());

    let state = AppState::new();

    seed_dev_data(&state)
        .await
        .expect("Failed to seed dev data");

    let session_store = FileSessionStorage::new();
    let session_layer = SessionManagerLayer::new(session_store);

    // use the service's user_store so that the data syncs
    let auth_layer =
        AuthManagerLayerBuilder::new(state.user_service.store().clone(), session_layer).build();

    let webauthn_routes = auth::router(state.user_service.store().clone())
        .expect("Failed to initialize WebAuthn routes");

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
        .layer(auth_layer);

    let app = app.with_state(state);

    // run our app with hyper
    println!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind the tcp address");
    axum::serve(listener, app.into_make_service())
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
