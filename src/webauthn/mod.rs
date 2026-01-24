mod authority;
mod me;
mod passkey;
mod signin;
mod signout;
mod signup;
pub mod user;

use axum::{
    Router,
    extract::Extension,
    routing::{get, post},
};
use axum_login::login_required;
use std::{env, sync::Arc};
use thiserror::Error;
use webauthn_rs::prelude::{Url, WebauthnBuilder, WebauthnError as WebauthnCoreError};

use passkey::MemoryPasskeyStore;
pub use user::MemoryUserStore;

pub type AuthSession = axum_login::AuthSession<MemoryUserStore>;

/// Errors that occur during WebAuthn router initialization/configuration.
/// These are startup-time errors, not request-handling errors.
#[derive(Error, Debug)]
pub enum WebauthnConfigError {
    #[error("WebAuthn initialization failed: {0}")]
    WebauthnInit(#[from] WebauthnCoreError),
    #[error("Invalid URL for WebAuthn origin: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("BASE_URL must have a valid host for WebAuthn rp_id")]
    InvalidHost,
}

pub fn router<S: Clone + Send + Sync + 'static>(
    user_store: Arc<MemoryUserStore>,
) -> Result<Router<S>, WebauthnConfigError> {
    // Get base URL from environment variable, defaulting to localhost:3000
    let base_url = env::var("RAILWAY_PUBLIC_DOMAIN")
        .ok()
        .map(|f| format!("https://{}", f))
        .unwrap_or_else(|| {
            env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
        });

    let webauthn_url = format!("{}/", base_url);

    // Parse the base URL to extract rp_id and rp_origin for WebAuthn security
    let rp_origin = Url::parse(&base_url)?;
    let rp_id = rp_origin
        .host_str()
        .ok_or(WebauthnConfigError::InvalidHost)?;

    // Create WebAuthn instance and passkey storage
    let webauthn = Arc::new(
        WebauthnBuilder::new(rp_id, &rp_origin)?
            .rp_name("Monkesto")
            .build()?,
    );
    let passkey_store = Arc::new(MemoryPasskeyStore::new());

    // Protected routes (require login)
    let protected_routes = Router::new()
        .route(
            "/me",
            get(me::me_get::<MemoryUserStore, MemoryPasskeyStore>),
        )
        .route(
            "/passkey",
            post(passkey::create_passkey_post::<MemoryUserStore, MemoryPasskeyStore>),
        )
        .route(
            "/passkey/{id}/delete",
            post(passkey::delete_passkey_post::<MemoryPasskeyStore>),
        )
        .route("/signout", get(signout::signout_get))
        .route("/signout", post(signout::signout_post))
        .route_layer(login_required!(MemoryUserStore, login_url = "/signin"));

    // Public routes (no login required)
    let public_routes = Router::new()
        .route(
            "/signin",
            get(signin::signin_get::<MemoryPasskeyStore>)
                .post(signin::signin_post::<MemoryUserStore, MemoryPasskeyStore>),
        )
        .route(
            "/signup",
            get(signup::signup_get)
                .post(signup::signup_post::<MemoryUserStore, MemoryPasskeyStore>),
        );

    Ok(public_routes
        .merge(protected_routes)
        .layer(Extension(webauthn_url))
        .layer(Extension(webauthn))
        .layer(Extension(user_store))
        .layer(Extension(passkey_store)))
}
