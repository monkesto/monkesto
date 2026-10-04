pub mod user_error;

use crate::authn::passkey::PasskeyError;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum_login::tracing::error;
use disintegrate::{BoxDynError, DecisionError};
use thiserror::Error;
pub use user_error::{UserError, UserResult};

#[derive(Error, Debug)]
pub enum AuthnError {
    #[error(transparent)]
    User(#[from] UserError),
    #[error(transparent)]
    Passkey(#[from] PasskeyError),
    #[error("got an error from the disintegrate event store: {0}")]
    DisintegrateEvent(BoxDynError),
    #[error("got an error from the disintegrate state store: {0}")]
    DisintegrateState(BoxDynError),
}

impl IntoResponse for AuthnError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::DisintegrateState(_) | Self::DisintegrateEvent(_) => {
                error!("{}", self);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternaleError, try again later",
                )
                    .into_response()
            }
            Self::User(err) => err.into_response(),
            Self::Passkey(err) => err.into_response(),
        }
    }
}

impl From<DecisionError<AuthnError>> for AuthnError {
    fn from(error: DecisionError<AuthnError>) -> Self {
        match error {
            DecisionError::EventStore(e) => AuthnError::DisintegrateEvent(e),
            DecisionError::StateStore(e) => AuthnError::DisintegrateState(e),
            DecisionError::Domain(e) => e,
        }
    }
}
