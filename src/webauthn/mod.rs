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
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::get,
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

/// Errors that occur during WebAuthn authentication flows.
/// These implement IntoResponse for use in route handlers.
#[derive(Error, Debug)]
pub enum WebauthnError {
    #[error("User Not Found")]
    UserNotFound,
    #[error("Authentication session expired")]
    SessionExpired,
    #[error("Invalid input data")]
    InvalidInput,
    #[error("Deserialising Session failed: {0}")]
    InvalidSessionState(#[from] tower_sessions::session::Error),
    #[error("Store operation failed: {0}")]
    StoreError(String),
    #[error("Login failed: {0}")]
    LoginFailed(String),
    #[error("Serialization failed: {0}")]
    SerializationError(#[from] serde_json::Error),
}

impl IntoResponse for WebauthnError {
    fn into_response(self) -> Response {
        match self {
            WebauthnError::SessionExpired => {
                Redirect::to("/signin?error=session_expired").into_response()
            }
            WebauthnError::InvalidInput => {
                (StatusCode::BAD_REQUEST, "Invalid Input").into_response()
            }
            WebauthnError::UserNotFound => {
                (StatusCode::NOT_FOUND, "User Not Found").into_response()
            }
            WebauthnError::InvalidSessionState(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Deserialising Session failed",
            )
                .into_response(),
            WebauthnError::StoreError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Store operation failed").into_response()
            }
            WebauthnError::LoginFailed(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Login failed").into_response()
            }
            WebauthnError::SerializationError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Serialization failed").into_response()
            }
        }
    }
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
            axum::routing::post(
                passkey::create_passkey_post::<MemoryUserStore, MemoryPasskeyStore>,
            ),
        )
        .route(
            "/passkey/{id}/delete",
            axum::routing::post(passkey::delete_passkey_post::<MemoryPasskeyStore>),
        )
        .route("/signout", get(signout::signout_get))
        .route("/signout", axum::routing::post(signout::signout_post))
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
