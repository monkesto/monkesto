use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;
use webauthn_rs::prelude::WebauthnError as WebauthnCoreError;

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
    #[error("WebAuthn initialization failed: {0}")]
    WebauthnInit(#[from] WebauthnCoreError),
    #[error("Invalid URL for WebAuthn origin: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("BASE_URL must have a valid host for WebAuthn rp_id")]
    InvalidHost,
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
                axum::response::Redirect::to("/webauthn/signin?error=session_expired")
                    .into_response()
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
            WebauthnError::WebauthnInit(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "WebAuthn initialization failed",
            )
                .into_response(),
            WebauthnError::InvalidUrl(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Invalid URL for WebAuthn origin",
            )
                .into_response(),
            WebauthnError::InvalidHost => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "BASE_URL must have a valid host for WebAuthn rp_id",
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
