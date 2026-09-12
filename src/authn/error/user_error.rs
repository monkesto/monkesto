use crate::authn::{AuthnService, UserId};
use crate::email::Email;
use crate::error::DecodeError;
use crate::error::DecodeError::FieldRequired;
use crate::proto::authn::error::user_error::ProtoUserError;
use crate::proto::authn::error::user_error::proto_user_error::UserErrorType;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum UserError {
    #[error("the email {0} already exists")]
    EmailConflict(Email),
    #[error("a user with the email: {0} doesn't exist")]
    EmailDoesntExist(Email),
    #[error("a user with the id {0} already exists")]
    IdCollision(UserId),
    #[error("no user exists with the provided id: {0}")]
    UserDoesntExist(UserId),
    #[error("there isn't any user associated with the current session")]
    SessionNotFound,
    #[error("sqlx returned an error: {0}")]
    Sqlx(String),
    #[error("failed to seed a dev user with the email {0}")]
    SeedFailure(Email),
    #[error("failed to decode a passkey: {0}")]
    PasskeyDecode(String),
    #[error("invalid input")]
    InvalidInput,
    #[error("failed to encode or decode a value with serde-json")]
    SerdeJson(String),
    #[error("failed to insert or retrieve a session credential: {0}")]
    Session(String),
    #[error("authentication failed")]
    AuthenticationFailed,
    #[error("failed to send an email, could not fine the resend API key")]
    MissingResendApiKey,
    #[error("failed to send an email with resend: {0}")]
    Resend(String),
    #[error("incorrect verification code")]
    InvalidVerificationCode,
}

pub type UserResult<T> = Result<T, UserError>;

impl From<sqlx::Error> for UserError {
    fn from(value: sqlx::Error) -> Self {
        Self::Sqlx(value.to_string())
    }
}

impl From<serde_json::Error> for UserError {
    fn from(value: serde_json::Error) -> Self {
        Self::SerdeJson(value.to_string())
    }
}

impl From<axum_login::Error<AuthnService>> for UserError {
    fn from(value: axum_login::Error<AuthnService>) -> Self {
        match value {
            axum_login::Error::Session(e) => Self::Session(e.to_string()),
            axum_login::Error::Backend(e) => e,
        }
    }
}

impl From<tower_sessions::session::Error> for UserError {
    fn from(value: tower_sessions::session::Error) -> Self {
        match value {
            tower_sessions::session::Error::SerdeJson(s) => UserError::SerdeJson(s.to_string()),
            tower_sessions::session::Error::Store(s) => UserError::Session(s.to_string()),
        }
    }
}

impl From<resend_rs::Error> for UserError {
    fn from(value: resend_rs::Error) -> Self {
        UserError::Resend(value.to_string())
    }
}

impl TryFrom<ProtoUserError> for UserError {
    type Error = DecodeError;

    fn try_from(e: ProtoUserError) -> Result<Self, Self::Error> {
        let user_error = match e.user_error_type.ok_or(FieldRequired)? {
            UserErrorType::EmailConflict(e) => UserError::EmailConflict(Email::try_new(e)?),
            UserErrorType::EmailDoesntExist(e) => UserError::EmailDoesntExist(Email::try_new(e)?),
            UserErrorType::IdCollision(id) => UserError::IdCollision(id.try_into()?),
            UserErrorType::UserDoesntExist(id) => UserError::UserDoesntExist(id.try_into()?),
            UserErrorType::SessionNotFound(_) => UserError::SessionNotFound,
            UserErrorType::Sqlx(e) => UserError::Sqlx(e),
            UserErrorType::SeedFailure(e) => UserError::SeedFailure(Email::try_new(e)?),
            UserErrorType::PasskeyDecode(s) => UserError::PasskeyDecode(s),
            UserErrorType::InvalidInput(_) => UserError::InvalidInput,
            UserErrorType::SerdeJson(s) => UserError::SerdeJson(s),

            UserErrorType::Session(s) => UserError::Session(s),
            UserErrorType::AuthenticationFailed(_) => UserError::AuthenticationFailed,
            UserErrorType::MissingResendApiKey(_) => UserError::MissingResendApiKey,
            UserErrorType::Resend(s) => UserError::Resend(s),
            UserErrorType::InvalidVerificationCode(_) => UserError::InvalidVerificationCode,
        };

        Ok(user_error)
    }
}

impl From<UserError> for ProtoUserError {
    fn from(e: UserError) -> Self {
        let err = match e {
            UserError::EmailConflict(em) => UserErrorType::EmailConflict(em.to_string()),
            UserError::EmailDoesntExist(em) => UserErrorType::EmailDoesntExist(em.to_string()),
            UserError::IdCollision(id) => UserErrorType::IdCollision(id.into()),
            UserError::UserDoesntExist(id) => UserErrorType::UserDoesntExist(id.into()),
            UserError::SessionNotFound => UserErrorType::SessionNotFound(()),
            UserError::Sqlx(s) => UserErrorType::Sqlx(s),
            UserError::SeedFailure(em) => UserErrorType::SeedFailure(em.to_string()),
            UserError::PasskeyDecode(s) => UserErrorType::PasskeyDecode(s),
            UserError::InvalidInput => UserErrorType::InvalidInput(()),
            UserError::SerdeJson(s) => UserErrorType::SerdeJson(s),
            UserError::Session(s) => UserErrorType::Session(s),
            UserError::AuthenticationFailed => UserErrorType::AuthenticationFailed(()),
            UserError::MissingResendApiKey => UserErrorType::MissingResendApiKey(()),
            UserError::Resend(s) => UserErrorType::Resend(s),
            UserError::InvalidVerificationCode => UserErrorType::InvalidVerificationCode(()),
        };

        Self {
            user_error_type: Some(err),
        }
    }
}
