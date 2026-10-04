use crate::authn::{AuthnService, UserId};
use crate::email::{Email, EmailError};
use crate::id::ident_error::IdentError;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum_login::tracing::error;
use disintegrate::{BoxDynError, DecisionError};
use std::backtrace::Backtrace;

#[derive(Debug, thiserror::Error)]
pub enum UserError {
    #[error("the email {0} already exists")]
    /// A user tried to sign up with an email that is already linked to an account
    ///
    /// The user should have already given a verification code by this point, so account numeration attacks shouldn't be an issue.
    EmailConflict(Email),
    #[error("a user with the email: {0} doesn't exist")]
    /// The email is not associated with a valid user id
    EmailDoesntExist(Email),
    #[error("unable to find a user with the supplied credential")]
    CredentialNotFound,
    #[error("a user with the id {0} already exists")]
    /// A user with the same id already exists
    ///
    /// The id is server-generated, so a form resubmission will likely fix the issue
    IdCollision(UserId),
    #[error("no user exists with the provided id: {0}")]
    UserDoesntExist(UserId),
    #[error("there isn't any user associated with the current session")]
    SessionNotFound,
    #[error("sqlx returned an error: {source}, backtrace {:#?}", backtrace)]
    Sqlx {
        #[from]
        source: sqlx::Error,
        backtrace: Backtrace,
    },
    #[error("failed to encode or decode a value with serde-json")]
    SerdeJson(#[from] serde_json::Error),
    #[error("failed to insert or retrieve a session credential: {0}")]
    Session(#[from] tower_sessions::session::Error),
    #[error("failed to send an email with resend: {0}")]
    Resend(#[from] resend_rs::Error),
    #[error("incorrect verification code")]
    InvalidVerificationCode,
    #[error("missing verification code")]
    MissingVerificationCode,
    #[error(transparent)]
    ParseUserId(#[from] IdentError),
    #[error(transparent)]
    ParseEmail(#[from] EmailError),
    #[error("got an error from the disintegrate event store: {0}")]
    DisintegrateEvent(BoxDynError),
    #[error("got an error from the disintegrate state store: {0}")]
    DisintegrateState(BoxDynError),
}

pub type UserResult<T> = Result<T, UserError>;

impl From<axum_login::Error<AuthnService>> for UserError {
    fn from(value: axum_login::Error<AuthnService>) -> Self {
        match value {
            axum_login::Error::Session(e) => Self::Session(e),
            axum_login::Error::Backend(e) => e,
        }
    }
}

impl From<DecisionError<UserError>> for UserError {
    fn from(error: DecisionError<UserError>) -> Self {
        match error {
            DecisionError::EventStore(e) => Self::DisintegrateEvent(e),
            DecisionError::StateStore(e) => Self::DisintegrateState(e),
            DecisionError::Domain(e) => e,
        }
    }
}

impl IntoResponse for UserError {
    fn into_response(self) -> axum::response::Response {
        use UserError::*;

        match self {
            Sqlx { .. }
            | SerdeJson { .. }
            | Session(_)
            | Resend(_)
            | DisintegrateState(_)
            | DisintegrateEvent(_) => {
                error!("{}", self);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Internal Error, Try again later.",
                )
                    .into_response()
            }

            EmailConflict(_) => StatusCode::CONFLICT.into_response(),
            EmailDoesntExist(_) => StatusCode::CONFLICT.into_response(),
            IdCollision(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Id Collision, try again.",
            )
                .into_response(),
            UserDoesntExist(_) => StatusCode::CONFLICT.into_response(),
            SessionNotFound => StatusCode::UNAUTHORIZED.into_response(),
            CredentialNotFound => StatusCode::UNAUTHORIZED.into_response(),
            InvalidVerificationCode => {
                (StatusCode::BAD_REQUEST, "Incorrect Verification Code").into_response()
            }
            MissingVerificationCode => {
                (StatusCode::BAD_REQUEST, "Missing Verification Code").into_response()
            }
            ParseUserId(_) => (StatusCode::BAD_REQUEST, "Invalid User Id").into_response(),
            ParseEmail(_) => (StatusCode::BAD_REQUEST, "Invalid Email").into_response(),
        }
    }
}
