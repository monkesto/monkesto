use crate::authn::PasskeyId;
use crate::id::ident_error::IdentError;
use axum::http::StatusCode;
use axum_login::tracing::error;
use disintegrate::{BoxDynError, DecisionError};
use std::backtrace::Backtrace;
use thiserror::Error;
use webauthn_rs::prelude::WebauthnError;

/// Errors that occur during passkey management operations.
#[derive(Error, Debug)]
pub enum PasskeyError {
    #[error("Session expired")]
    SessionExpired,
    #[error("Session error: {0}")]
    Session(#[from] tower_sessions::session::Error),
    #[error("a passkey with the id {0} already exists")]
    IdConflict(PasskeyId),
    #[error("no passkey exists with the provided id: {0}")]
    PasskeyDoesntExist(PasskeyId),
    #[error("failed to serialize a value with serde_json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("received an error from sqlx: {source}; backtrace: {:#?}", backtrace)]
    Sqlx {
        #[from]
        source: sqlx::Error,
        backtrace: Backtrace,
    },
    #[error(
        "received an error from webauthn: {source}; backtrace: {:#?}",
        backtrace
    )]
    Webauthn {
        #[from]
        source: WebauthnError,
        backtrace: Backtrace,
    },
    #[error("missing a credential field in the signin form")]
    MissingCredential,
    #[error("failed to create a passkey_id: {0}")]
    Ident(#[from] IdentError),
    #[error("disintegrate event store returned an error: {0}")]
    DisintegrateEvent(BoxDynError),
    #[error("disintegrate state store returned an error: {0}")]
    DisintegrateState(BoxDynError),
}

impl From<DecisionError<PasskeyError>> for PasskeyError {
    fn from(err: DecisionError<PasskeyError>) -> Self {
        match err {
            DecisionError::EventStore(e) => Self::DisintegrateEvent(e),
            DecisionError::StateStore(e) => Self::DisintegrateState(e),
            DecisionError::Domain(e) => e,
        }
    }
}

impl axum::response::IntoResponse for PasskeyError {
    fn into_response(self) -> axum::response::Response {
        use PasskeyError::*;

        match self {
            Session(_)
            | Json(_)
            | Sqlx { .. }
            | Webauthn { .. }
            | DisintegrateEvent(_)
            | DisintegrateState(_) => {
                error!("{}", self);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            SessionExpired => StatusCode::UNAUTHORIZED.into_response(),
            IdConflict(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Id Collision, try again.",
            )
                .into_response(),
            PasskeyDoesntExist(_) => StatusCode::CONFLICT.into_response(),
            MissingCredential => StatusCode::BAD_REQUEST.into_response(),
            Ident(_) => (StatusCode::BAD_REQUEST, "Invalid Passkey Id").into_response(),
        }
    }
}
