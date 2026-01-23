mod ident;
mod journal;
mod known_errors;
mod maud_header;
mod notfoundpage;
mod webauthn;

use axum::Router;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::routing::get;
use axum_login::{AuthManagerLayerBuilder, login_required};
use dotenvy::dotenv;
use std::env;

use std::sync::Arc;
use tower_http::services::ServeFile;
use tower_sessions::SessionManagerLayer;
use tower_sessions_file_store::FileSessionStorage;

use crate::journal::JournalMemoryStore;
use crate::journal::transaction::TransasctionMemoryStore;
use crate::webauthn::MemoryUserStore;

pub type AuthSession = axum_login::AuthSession<MemoryUserStore>;

#[derive(Clone)]
pub struct AppState {
    user_store: Arc<MemoryUserStore>,
    journal_store: JournalMemoryStore,
}

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

    let user_store = Arc::new(MemoryUserStore::new());
    user_store.seed_dev_users().await;

    let session_store = FileSessionStorage::new();
    let session_layer = SessionManagerLayer::new(session_store);
    let auth_layer = AuthManagerLayerBuilder::new((*user_store).clone(), session_layer).build();

    let webauthn_routes =
        webauthn::router(user_store.clone()).expect("Failed to initialize WebAuthn router");

    let journal_routes = Router::new()
        .route("/journal", get(journal::views::homepage::journal_list))
        .route(
            "/createjournal",
            axum::routing::post(journal::commands::create_journal),
        )
        .route(
            "/journal/{id}",
            get(journal::views::homepage::journal_detail),
        )
        .route(
            "/journal/{id}/transaction",
            get(journal::views::transaction::transaction_list_page),
        )
        .route(
            "/journal/{id}/transaction",
            axum::routing::post(journal::commands::transact),
        )
        .route(
            "/journal/{id}/account",
            get(journal::views::account::account_list_page),
        )
        .route(
            "/journal/{id}/person",
            get(journal::views::person::people_list_page),
        )
        .route(
            "/journal/{id}/invite",
            axum::routing::post(journal::commands::invite_user),
        )
        .route(
            "/journal/{id}/createaccount",
            axum::routing::post(journal::commands::create_account),
        )
        .route_layer(login_required!(MemoryUserStore, login_url = "/signin"));

    // the dockerfile defines this for production deployments
    let site_root = std::env::var("SITE_ROOT").unwrap_or_else(|_| "target/site".to_string());

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
        .with_state(AppState {
            user_store,
            journal_store: JournalMemoryStore::new(Arc::new(TransasctionMemoryStore::new())),
        });

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
