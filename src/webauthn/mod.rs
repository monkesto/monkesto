mod error;
mod login;
mod register;
mod startup;

use axum::{
    Router,
    extract::Extension,
    response::{IntoResponse, Redirect},
    routing::get,
};
use std::env;
use tower_sessions::{
    Expiry, MemoryStore, SessionManagerLayer,
    cookie::{SameSite, time::Duration},
};

use error::WebauthnError;
use startup::AppState;

pub fn router<S: Clone + Send + Sync + 'static>() -> Result<Router<S>, WebauthnError> {
    // Get base URL from environment variable, defaulting to localhost:3000
    let base_url = env::var("RAILWAY_PUBLIC_DOMAIN")
        .ok()
        .map(|f| format!("https://{}", f))
        .unwrap_or_else(|| {
            env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
        });

    let webauthn_url = format!("{}/webauthn/", base_url);

    // Parse the base URL to extract rp_id and rp_origin for WebAuthn security
    let rp_origin = webauthn_rs::prelude::Url::parse(&base_url)?;
    let rp_id = rp_origin.host_str().ok_or(WebauthnError::InvalidHost)?;

    let app_state = AppState::new(rp_id, rp_origin.clone())?;

    Ok(Router::new()
        .route("/", get(redirect_to_login))
        .route("/login", get(login::login_get).post(login::login_post))
        .route(
            "/register",
            get(register::register_get).post(register::register_post),
        )
        .layer(Extension(webauthn_url))
        .layer(Extension(app_state))
        .layer(
            SessionManagerLayer::new(MemoryStore::default())
                .with_name("webauthnrs")
                .with_same_site(SameSite::Lax)
                .with_secure(true)
                .with_expiry(Expiry::OnInactivity(Duration::seconds(360))),
        ))
}

async fn redirect_to_login() -> impl IntoResponse {
    Redirect::permanent("/webauthn/login")
}
