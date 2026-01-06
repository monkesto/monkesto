use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use thiserror::Error;
use webauthn_rs::prelude::WebauthnError as WebauthnCoreError;

#[derive(Error, Debug)]
pub enum WebauthnError {
    #[error("unknown webauthn error")]
    Unknown,
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
}
impl IntoResponse for WebauthnError {
    fn into_response(self) -> Response {
        match self {
            WebauthnError::SessionExpired => {
                axum::response::Redirect::to("/webauthn/login?error=session_expired")
                    .into_response()
            }
            WebauthnError::InvalidInput => {
                (StatusCode::BAD_REQUEST, "Invalid Input").into_response()
            }
            _ => {
                let body = match self {
                    WebauthnError::UserNotFound => "User Not Found",
                    WebauthnError::Unknown => "Unknown Error",
                    WebauthnError::InvalidSessionState(_) => "Deserialising Session failed",
                    WebauthnError::WebauthnInit(_) => "WebAuthn initialization failed",
                    WebauthnError::InvalidUrl(_) => "Invalid URL for WebAuthn origin",
                    WebauthnError::InvalidHost => {
                        "BASE_URL must have a valid host for WebAuthn rp_id"
                    }
                    _ => unreachable!(),
                };
                (StatusCode::INTERNAL_SERVER_ERROR, body).into_response()
            }
        }
    }
}
