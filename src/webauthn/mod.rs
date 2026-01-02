use axum::{
    Router,
    extract::Extension,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use tower_sessions::{
    Expiry, MemoryStore, SessionManagerLayer,
    cookie::{SameSite, time::Duration},
};

mod error;
/*
 * Webauthn RS server side tutorial.
 */

// The handlers that process the data can be found in the auth.rs file
// This file contains the wasm client loading code and the axum routing
use auth::{finish_authentication, finish_register, start_authentication, start_register};
use startup::AppState;

// Moved to src/main.rs
//
// #[macro_use]
// extern crate tracing;

mod auth;
mod startup;

// 7. That's it! The user has now authenticated!

// =======
// Below is glue/stubs that are needed to make the above work, but don't really affect
// the work flow too much.

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    // Create the app
    let app_state = AppState::new();

    let session_store = MemoryStore::default();

    // build our application with a route
    let app = Router::new()
        .route("/register_start/{username}", post(start_register))
        .route("/register_finish", post(finish_register))
        .route("/login_start/{username}", post(start_authentication))
        .route("/login_finish", post(finish_authentication))
        .route("/", get(serve_auth_html))
        .route("/auth.js", get(serve_auth_js))
        .layer(Extension(app_state))
        .layer(
            SessionManagerLayer::new(session_store)
                .with_name("webauthnrs")
                .with_same_site(SameSite::Strict)
                .with_secure(false) // TODO: change this to true when running on an HTTPS/production server instead of locally
                .with_expiry(Expiry::OnInactivity(Duration::seconds(360))),
        )
        .fallback(handler_404);

    Router::new().merge(app)
}

async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "nothing to see here")
}

async fn serve_auth_html() -> impl IntoResponse {
    const HTML_CONTENT: &str = include_str!("auth.html");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        HTML_CONTENT,
    )
}

async fn serve_auth_js() -> impl IntoResponse {
    const JS_CONTENT: &str = include_str!("auth.js");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/javascript")],
        JS_CONTENT,
    )
}
