mod auth;
mod error;
mod startup;
//mod login;
//mod register;

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

use auth::{finish_authentication, finish_register, start_authentication, start_register};
use startup::AppState;

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/register_start/{username}", post(start_register))
        .route("/register_finish", post(finish_register))
        .route("/login_start/{username}", post(start_authentication))
        .route("/login_finish", post(finish_authentication))
        //.route("/login", get(login::login))
        //.route("/register", get(register::register))
        .route("/auth.js", get(serve_auth_js))
        .layer(Extension(AppState::new()))
        .layer(
            SessionManagerLayer::new(MemoryStore::default())
                .with_name("webauthnrs")
                .with_same_site(SameSite::Lax)
                .with_secure(true)
                .with_expiry(Expiry::OnInactivity(Duration::seconds(360))),
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
