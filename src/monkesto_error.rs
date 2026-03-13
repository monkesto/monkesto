use crate::account::AccountStoreError;
use crate::auth::user::EmailError;
use crate::auth::user::UserStoreError;
use crate::ident::IdentError;
use crate::journal::JournalStoreError;
use crate::name::NameError;
use crate::transaction::TransactionStoreError;
use axum::response::Redirect;
use base64::Engine;
use base64::engine::general_purpose;
use postcard::to_allocvec;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize, Eq, PartialEq)]
pub enum MonkestoError {
    #[error("failed to decode an error")]
    Decode,

    #[error("failed to create a name: {0}")]
    NameCreation(#[from] NameError),

    #[error("failed to create an Ident: {0}")]
    IdentCreation(#[from] IdentError),

    #[error("failed to create an email: {0}")]
    EmailCreation(String),

    #[error("an error was returned from the journal store: {0}")]
    JournalStore(#[from] JournalStoreError),

    #[error("an error was returned from the user store: {0}")]
    UserStore(#[from] UserStoreError),

    #[error("an error was returned from the account store: {0}")]
    AccountStore(#[from] AccountStoreError),

    #[error("an error was returned from the transaction store: {0}")]
    TransactionStore(#[from] TransactionStoreError),
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

// we have to implement this manually because EmailError doesn't implement deserialize
impl From<EmailError> for MonkestoError {
    fn from(err: EmailError) -> Self {
        Self::EmailCreation(err.to_string())
    }
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
