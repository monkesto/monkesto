mod error;
mod passkey;
mod signin;
mod signout;
mod signup;
mod startup;
mod storage;
pub mod user;
mod authority;

use axum::{
    Router,
    extract::Extension,
    response::{IntoResponse, Redirect},
    routing::get,
};
use std::{env, sync::Arc};
use tower_sessions::{
    Expiry, MemoryStore, SessionManagerLayer,
    cookie::{SameSite, time::Duration},
};
use webauthn_rs::prelude::Url;

use error::WebauthnError;
use startup::AppState;
use storage::{UserStorage, memory::MemoryStorage};

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
    let rp_origin = Url::parse(&base_url)?;
    let rp_id = rp_origin.host_str().ok_or(WebauthnError::InvalidHost)?;

    // Create WebAuthn instance and default storage
    let webauthn = AppState::build_webauthn(rp_id, &rp_origin)?;
    let storage = Arc::new(MemoryStorage::new()) as Arc<dyn UserStorage>;

    // Create AppState with components
    let app_state = AppState::new(webauthn, storage);

    Ok(Router::new()
        .route("/", get(redirect_to_signin))
        .route("/signin", get(signin::signin_get).post(signin::signin_post))
        .route("/signup", get(signup::signup_get).post(signup::signup_post))
        .route(
            "/passkey",
            get(passkey::passkey_get).post(passkey::create_passkey_post),
        )
        .route(
            "/passkey/{id}/delete",
            axum::routing::post(passkey::delete_passkey_post),
        )
        .route("/signout", get(signout::signout_get))
        .route("/signout", axum::routing::post(signout::signout_post))
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

async fn redirect_to_signin() -> impl IntoResponse {
    Redirect::temporary("/webauthn/signin")
}
