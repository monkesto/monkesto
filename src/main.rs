mod account;
mod auth;
mod authority;
mod event;
mod ident;
mod journal;
mod known_errors;
mod notfoundpage;
mod seed;
mod service;
mod theme;
mod transaction;

use axum::Router;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::routing::get;
use axum_login::AuthManagerLayerBuilder;
use dotenvy::dotenv;
use std::env;

use crate::auth::MemoryUserStore;
use crate::service::MemoryService;
use seed::seed_dev_data;
use tower_http::services::ServeFile;
use tower_sessions::SessionManagerLayer;
use tower_sessions_file_store::FileSessionStorage;

type StateType = MemoryService;

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

    // this handles creation of all the stores journal stores and the base user store
    let service: MemoryService = MemoryService::default();

    // this will seed the users and journals
    seed_dev_data(&service)
        .await
        .expect("Failed to seed dev data");

    let session_store = FileSessionStorage::new();
    let session_layer = SessionManagerLayer::new(session_store);

    // use the service's user_store so that the data syncs
    let auth_layer =
        AuthManagerLayerBuilder::new(service.user_store().clone(), session_layer).build();

    let webauthn_routes =
        auth::router(service.user_store().clone()).expect("Failed to initialize WebAuthn routes");

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

    let app = app.with_state(service);

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
