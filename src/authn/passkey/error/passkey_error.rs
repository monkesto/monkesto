use crate::authn::{PasskeyId, UserId};
use crate::error::DecodeError;
use crate::error::DecodeError::FieldRequired;
use crate::proto::authn::passkey::error::passkey_error::ProtoPasskeyError;
use crate::proto::authn::passkey::error::passkey_error::proto_passkey_error::PasskeyErrorType;
use thiserror::Error;
use tower_sessions::session::Error;

/// Errors that occur during passkey management operations.
#[derive(Error, Debug)]
pub enum PasskeyError {
    #[error("Session expired")]
    SessionExpired,
    #[error("Invalid input data")]
    InvalidInput,
    #[error("Session error: {0}")]
    SessionError(String),
    #[error("a passkey with the id {0} already exists")]
    IdConflict(PasskeyId),
    #[error("no passkey exists with the provided id: {0}")]
    PasskeyDoesntExist(PasskeyId),
    #[error("no user exists with the provided id: {0}")]
    UserDoesntExist(UserId),
    #[error("failed to serialize a value with serde_json")]
    Json(String),
    #[error("received an error from sqlx: {0}")]
    Sqlx(String),
}

impl From<tower_sessions::session::Error> for PasskeyError {
    fn from(err: tower_sessions::session::Error) -> Self {
        match err {
            Error::SerdeJson(s) => Self::Json(s.to_string()),
            Error::Store(s) => Self::SessionError(s.to_string()),
        }
    }
}

impl From<serde_json::Error> for PasskeyError {
    fn from(err: serde_json::Error) -> Self {
        Self::Json(err.to_string())
    }
}

impl From<sqlx::Error> for PasskeyError {
    fn from(err: sqlx::Error) -> Self {
        Self::Sqlx(err.to_string())
    }
}

impl TryFrom<ProtoPasskeyError> for PasskeyError {
    type Error = DecodeError;

    fn try_from(e: ProtoPasskeyError) -> Result<Self, Self::Error> {
        let passkey_error = match e.passkey_error_type.ok_or(FieldRequired)? {
            PasskeyErrorType::SessionExpired(_) => PasskeyError::SessionExpired,

            PasskeyErrorType::InvalidInput(_) => PasskeyError::InvalidInput,

            PasskeyErrorType::SessionError(s) => PasskeyError::SessionError(s),
            PasskeyErrorType::IdConflict(id) => PasskeyError::IdConflict(id.try_into()?),
            PasskeyErrorType::PasskeyDoesntExist(id) => {
                PasskeyError::PasskeyDoesntExist(id.try_into()?)
            }
            PasskeyErrorType::UserDoesntExist(id) => PasskeyError::UserDoesntExist(id.try_into()?),
            PasskeyErrorType::Json(s) => PasskeyError::Json(s),
            PasskeyErrorType::Sqlx(s) => PasskeyError::Sqlx(s),
        };

        Ok(passkey_error)
    }
}

impl From<PasskeyError> for ProtoPasskeyError {
    fn from(e: PasskeyError) -> Self {
        let err = match e {
            PasskeyError::SessionExpired => PasskeyErrorType::SessionExpired(()),
            PasskeyError::InvalidInput => PasskeyErrorType::InvalidInput(()),
            PasskeyError::SessionError(s) => PasskeyErrorType::SessionError(s),
            PasskeyError::IdConflict(id) => PasskeyErrorType::IdConflict(id.into()),
            PasskeyError::PasskeyDoesntExist(id) => PasskeyErrorType::PasskeyDoesntExist(id.into()),
            PasskeyError::UserDoesntExist(id) => PasskeyErrorType::UserDoesntExist(id.into()),
            PasskeyError::Json(s) => PasskeyErrorType::Json(s),
            PasskeyError::Sqlx(s) => PasskeyErrorType::Sqlx(s),
        };

        Self {
            passkey_error_type: Some(err),
        }
    }
}
