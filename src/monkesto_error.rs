use crate::authn::passkey::PasskeyError;
use crate::authn::user::UserError;
use crate::email::EmailError;
use crate::id::IdentError;
use crate::journal::JournalError;
use crate::name::NameError;
use crate::proto::error::ProtoMonkestoError;
use crate::serde::error::ProtoError;
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
    Proto(#[from] ProtoError),

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
            proto_error.try_into().unwrap_or_else(MonkestoError::Proto)
        } else {
            MonkestoError::Proto(ProtoError::Deserialize)
        }
    }
}

#[derive(Deserialize)]
pub struct UrlError {
    pub err: Option<String>,
    #[expect(dead_code)]
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
