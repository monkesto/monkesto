mod authn;
mod authority;
mod authz;
mod email;
mod entitlement;
mod event_id;
mod id;
mod journal;
mod monkesto_error;
pub mod name;
mod notfoundpage;
mod seed;
mod serde;
mod status;
mod theme;
mod time_provider;
pub mod util;

use crate::authn::{AuthnEventStore, AuthnService};
use crate::authz::{AuthzEventStore, AuthzService, RoleIndex};
use crate::journal::JournalService;
use crate::journal::store::JournalEventStore;
use axum::Router;
use axum::extract::FromRef;
use axum::http::header;
use axum::http::{Response, StatusCode};
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::routing::get;
use axum_login::tracing::{Level, Span};
use axum_login::{AuthManagerLayerBuilder, tracing};
use dotenvy::dotenv;
use journal::{account, transaction};
use seed::seed_dev_data;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::env;
use std::time::Duration;
use tokio::signal;
use tower_http::services::ServeFile;
use tower_http::trace::TraceLayer;
use tower_sessions::SessionManagerLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

pub mod proto {
    pub mod error {
        include!(concat!(env!("OUT_DIR"), "/proto.error.rs"));
    }
}

#[derive(Clone)]
struct AppState {
    authn_service: AuthnService,
    journal_service: JournalService,
    authz_service: AuthzService,
}

impl AppState {
    fn new(
        authn_service: AuthnService,
        authz_service: AuthzService,
        journal_service: JournalService,
    ) -> Self {
        Self {
            authn_service,
            journal_service,
            authz_service,
        }
    }
}

impl FromRef<AppState> for JournalService {
    fn from_ref(input: &AppState) -> Self {
        input.journal_service.clone()
    }
}

impl FromRef<AppState> for AuthzService {
    fn from_ref(state: &AppState) -> Self {
        state.authz_service.clone()
    }
}

type StateType = AppState;
type BackendType = AuthnService;

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

    let database_url = env::var("DATABASE_URL").expect("failed to fetch database url");

    let public_pool = PgPool::connect(&database_url)
        .await
        .expect("failed to create pgpool");

    sqlx::query!("CREATE SCHEMA IF NOT EXISTS authz")
        .execute(&public_pool)
        .await
        .expect("failed to create the authz schema");

    sqlx::query!("CREATE SCHEMA IF NOT EXISTS authn")
        .execute(&public_pool)
        .await
        .expect("failed to create the authn schema");

    sqlx::query!("CREATE SCHEMA IF NOT EXISTS journal")
        .execute(&public_pool)
        .await
        .expect("failed to create the journal schema");

    let authn_pool = PgPoolOptions::new()
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query!("SET search_path TO authn")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&database_url)
        .await
        .expect("failed to create an authz pool");

    let session_store = tower_sessions_sqlx_store::PostgresStore::new(authn_pool.clone());
    session_store
        .migrate()
        .await
        .expect("failed to migrate session store");
    let session_layer = SessionManagerLayer::new(session_store);

    let auth_event_store = AuthnEventStore::try_new(authn_pool.clone())
        .await
        .expect("failed to create an auth event store");
    let auth_service = AuthnService::try_new(authn_pool.clone(), &auth_event_store)
        .await
        .expect("failed to create a projection pool");

    tokio::spawn(authn::event_listener(
        auth_event_store.clone(),
        auth_service.clone(),
    ));

    let journal_pool = PgPoolOptions::new()
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query!("SET search_path TO journal")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&database_url)
        .await
        .expect("failed to create a journal pool");

    let journal_event_store = JournalEventStore::try_new(journal_pool.clone())
        .await
        .expect("failed to create a journal event store");

    let journal_service =
        JournalService::try_new(journal_pool.clone(), journal_event_store.clone())
            .await
            .expect("failed to create a journal service");

    tokio::spawn(journal::domain::event_listener(
        journal_event_store,
        journal_service.clone(),
    ));

    // Disintegrate uses unqualified object names and cannot target a schema directly, so
    // authz needs a schema-scoped pool. Ideally, the backend would qualify its objects with
    // a configured schema, allowing isolated event stores to share a pool.
    let authz_pool = PgPoolOptions::new()
        .after_connect(|connection, _| {
            Box::pin(async move {
                sqlx::query!("SET search_path TO authz")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(&database_url)
        .await
        .expect("failed to create an authz pool");

    let authz_event_store = AuthzEventStore::try_new(authz_pool.clone())
        .await
        .expect("failed to create an authz event store");

    let role_index = RoleIndex::try_new(authz_pool, authz_event_store.clone())
        .await
        .expect("failed to create the role index");

    let authz_service = AuthzService::new(authz_event_store, role_index);

    let state = AppState::new(auth_service.clone(), authz_service, journal_service);

    seed_dev_data(&state)
        .await
        .expect("Failed to seed dev data");

    // use the service's user_store so that the data syncs
    let auth_layer = AuthManagerLayerBuilder::new(auth_service.clone(), session_layer).build();

    let webauthn_routes =
        authn::router(auth_service.clone()).expect("Failed to initialize WebAuthn routes");

    let journal_routes = journal::router()
        .merge(account::router())
        .merge(transaction::router())
        .merge(authz::router());

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
