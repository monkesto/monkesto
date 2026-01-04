mod auth;
mod error;
mod startup;

use axum::{
    Router,
    extract::Extension,
    http::{StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use maud::{DOCTYPE, Markup, html};
use std::env;
use tower_sessions::{
    Expiry, MemoryStore, SessionManagerLayer,
    cookie::{SameSite, time::Duration},
};

use auth::{finish_authentication, finish_register, start_authentication, start_register};
use startup::AppState;

use crate::maud_header::header;

pub fn router<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/register_start/{username}", post(start_register))
        .route("/register_finish", post(finish_register))
        .route("/login_start/{username}", post(start_authentication))
        .route("/login_finish", post(finish_authentication))
        .route("/", get(serve_auth_html))
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

fn auth_page() -> Markup {
    header(html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "WebAuthn-rs Tutorial" }
                script
                    src="https://cdn.jsdelivr.net/npm/js-base64@3.7.4/base64.min.js"
                    crossorigin="anonymous" {}
                script src="auth.js" async {}
                meta name="webauthn_url" content=(format!("{}/webauthn/", env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())));
            }
            body {
                p { "Welcome to the WebAuthn Server!" }

                div {
                    input
                        type="text"
                        id="username"
                        placeholder="Enter your username here";
                    button onclick="register()" { "Register" }
                    button onclick="login()" { "Login" }
                }

                div {
                    p id="flash_message" {}
                }
            }
        }
    })
}

async fn serve_auth_html() -> impl IntoResponse {
    let markup = auth_page();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        markup,
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
