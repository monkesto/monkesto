use crate::authn::user::UserError;
use crate::email::EmailError;
use crate::id::IdentError;
use crate::journal::JournalError;
use crate::name::NameError;
use axum::response::Redirect;
use base64::Engine;
use base64::engine::general_purpose;
use disintegrate::DecisionError;
use postcard::to_allocvec;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize, PartialEq)]
pub enum MonkestoError {
    #[error("failed to decode an error")]
    Decode,

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

impl MonkestoError {
    pub fn encode(&self) -> String {
        // to_allocvec should be infallible
        let bytes = to_allocvec(self).expect("postcard error serialization failed");

        general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    }

    pub fn redirect(&self, page: &str) -> Redirect {
        Redirect::to(&format!("{}?err={}", page, self.encode()))
    }

    pub fn decode(err: &str) -> Self {
        general_purpose::URL_SAFE_NO_PAD
            .decode(err)
            .ok()
            .and_then(|bytes| postcard::from_bytes(&bytes).ok())
            .unwrap_or(Self::Decode)
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
