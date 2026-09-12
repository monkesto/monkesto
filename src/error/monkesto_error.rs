use crate::authn::passkey::PasskeyError;
use crate::authn::user::UserError;
use crate::email::EmailError;
use crate::error::DecodeError;
use crate::error::DecodeError::FieldRequired;
use crate::id::ident_error::IdentError;
use crate::journal::error::journal_error::JournalError;
use crate::name::name_error::NameError;
use crate::proto::error::monkesto_error::ProtoMonkestoError;
use crate::proto::error::monkesto_error::proto_monkesto_error::MonkestoErrorType;
use axum::response::Redirect;
use base64::Engine;
use base64::engine::{DecodePaddingMode, GeneralPurposeConfig, simd};
use disintegrate::DecisionError;
use prost::Message;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum MonkestoError {
    #[error("failed to decode an error")]
    Decode(#[from] DecodeError),

    #[error("failed to create a name: {0}")]
    NameCreation(#[from] NameError),

    #[error("failed to create an Ident: {0}")]
    IdentCreation(#[from] IdentError),

    #[error("failed to create an email: {0}")]
    EmailCreation(#[from] EmailError),

    #[error("an error was returned from the journal store: {0}")]
    Journal(#[from] JournalError),

    #[error("an error was returned from the user store: {0}")]
    User(#[from] UserError),

    #[error("an error was returned from the passkey store")]
    Passkey(#[from] PasskeyError),

    #[error("the disintegrate event store returned an error: {0}")]
    DisintegrateEvent(String),

    #[error("the disintegrate state store returned an error: {0}")]
    DisintegrateState(String),
}

impl From<DecisionError<JournalError>> for MonkestoError {
    fn from(value: DecisionError<JournalError>) -> Self {
        match value {
            DecisionError::EventStore(e) => Self::DisintegrateEvent(e.to_string()),
            DecisionError::StateStore(e) => Self::DisintegrateState(e.to_string()),
            DecisionError::Domain(e) => Self::Journal(e),
        }
    }
}

impl From<DecisionError<UserError>> for MonkestoError {
    fn from(value: DecisionError<UserError>) -> Self {
        match value {
            DecisionError::EventStore(e) => Self::DisintegrateEvent(e.to_string()),
            DecisionError::StateStore(e) => Self::DisintegrateState(e.to_string()),
            DecisionError::Domain(e) => Self::User(e),
        }
    }
}

impl From<DecisionError<PasskeyError>> for MonkestoError {
    fn from(value: DecisionError<PasskeyError>) -> Self {
        match value {
            DecisionError::EventStore(e) => Self::DisintegrateEvent(e.to_string()),
            DecisionError::StateStore(e) => Self::DisintegrateState(e.to_string()),
            DecisionError::Domain(e) => Self::Passkey(e),
        }
    }
}

impl MonkestoError {
    pub fn redirect(self, page: &str) -> Redirect {
        let bytes = ProtoMonkestoError::from(self).encode_to_vec();
        Redirect::to(&format!(
            "{}?err={}",
            page,
            simd::Simd::standard(GeneralPurposeConfig::new().with_encode_padding(false))
                .encode(bytes)
        ))
    }

    pub fn decode(err: &str) -> Self {
        if let Some(Ok(proto_error)) = simd::Simd::standard(
            GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::RequireNone),
        )
        .decode(err)
        .ok()
        .map(|bytes| ProtoMonkestoError::decode(bytes.as_slice()))
        {
            proto_error.try_into().unwrap_or_else(MonkestoError::Decode)
        } else {
            MonkestoError::Decode(DecodeError::Deserialize)
        }
    }
}

#[derive(Deserialize)]
pub struct UrlError {
    pub err: Option<String>,
    pub next: Option<String>,
}

pub type MonkestoResult<T> = Result<T, MonkestoError>;

pub trait OrRedirect<T> {
    fn or_redirect(self, redirect_url: &str) -> Result<T, Redirect>;
}

impl<T, E: Into<MonkestoError>> OrRedirect<T> for Result<T, E> {
    fn or_redirect(self, redirect_url: &str) -> Result<T, Redirect> {
        self.map_err(|e| e.into().redirect(redirect_url))
    }
}

impl TryFrom<ProtoMonkestoError> for MonkestoError {
    type Error = DecodeError;

    fn try_from(proto_error: ProtoMonkestoError) -> Result<Self, Self::Error> {
        let error = match proto_error.monkesto_error_type.ok_or(FieldRequired)? {
            MonkestoErrorType::ErrorDecode(e) => MonkestoError::Decode(e.try_into()?),
            MonkestoErrorType::NameCreation(e) => MonkestoError::NameCreation(e.try_into()?),
            MonkestoErrorType::IdentCreation(e) => MonkestoError::IdentCreation(e.try_into()?),
            MonkestoErrorType::EmailCreation(e) => {
                MonkestoError::EmailCreation(EmailError::RegexViolated(e))
            }
            MonkestoErrorType::Journal(e) => MonkestoError::Journal(e.try_into()?),

            MonkestoErrorType::User(e) => MonkestoError::User(e.try_into()?),

            MonkestoErrorType::DisintegrateEvent(s) => MonkestoError::DisintegrateEvent(s),
            MonkestoErrorType::DisintegrateState(s) => MonkestoError::DisintegrateState(s),
            MonkestoErrorType::Passkey(e) => MonkestoError::Passkey(e.try_into()?),
        };

        Ok(error)
    }
}
impl From<MonkestoError> for ProtoMonkestoError {
    fn from(error: MonkestoError) -> Self {
        let e = match error {
            MonkestoError::Decode(e) => MonkestoErrorType::ErrorDecode(e.into()),
            MonkestoError::NameCreation(e) => MonkestoErrorType::NameCreation(e.into()),
            MonkestoError::IdentCreation(e) => MonkestoErrorType::IdentCreation(e.into()),
            MonkestoError::EmailCreation(EmailError::RegexViolated(s)) => {
                MonkestoErrorType::EmailCreation(s)
            }
            MonkestoError::Journal(e) => MonkestoErrorType::Journal(e.into()),

            MonkestoError::User(e) => MonkestoErrorType::User(e.into()),
            MonkestoError::DisintegrateEvent(s) => MonkestoErrorType::DisintegrateEvent(s),
            MonkestoError::DisintegrateState(s) => MonkestoErrorType::DisintegrateState(s),
            MonkestoError::Passkey(e) => MonkestoErrorType::Passkey(e.into()),
        };

        ProtoMonkestoError {
            monkesto_error_type: Some(e),
        }
    }
}
