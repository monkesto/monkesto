use crate::journal::{BalanceUpdate, Permissions};
use axum::response::Redirect;
use base64::{Engine, engine::general_purpose};
use postcard::to_allocvec;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub enum KnownErrors {
    InternalError {
        context: String,
    },

    SessionIdNotFound,

    UsernameNotFound {
        username: String,
    },

    InvalidUsername {
        username: String,
    },

    LoginFailed {
        username: String,
    },

    SignupPasswordMismatch {
        username: String,
    },

    UserDoesntExist,

    UserExists {
        username: String,
    },

    AccountExists,

    BalanceMismatch {
        attempted_transaction: Vec<BalanceUpdate>,
    },

    PermissionError {
        required_permissions: Permissions,
    },

    InvalidInput,

    InvalidId,

    NoInvitation,

    NotLoggedIn,

    UserCanAccessJournal,

    InvalidJournal,

    None,
}

impl KnownErrors {
    pub fn to_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn encode(&self) -> String {
        // to_allocvec should be infallible
        let bytes = to_allocvec(self).expect("postcard error serialization failed");

        general_purpose::URL_SAFE.encode(bytes)
    }

    pub fn redirect(&self, page: &str) -> Redirect {
        Redirect::to(&format!("{}?err={}", page, self.encode()))
    }

    pub fn decode(err: &str) -> Self {
        general_purpose::URL_SAFE
            .decode(err)
            .ok()
            .and_then(|bytes| postcard::from_bytes(&bytes).ok())
            .unwrap_or(Self::None)
    }
}

pub trait RedirectOnError<T> {
    fn or_redirect(self, error: KnownErrors, page: &str) -> Result<T, Redirect>;
}

impl<T, E> RedirectOnError<T> for Result<T, E> {
    fn or_redirect(self, error: KnownErrors, page: &str) -> Result<T, Redirect> {
        self.map_err(|_| error.redirect(page))
    }
}

#[derive(Deserialize)]
pub struct UrlError {
    pub err: Option<String>,
}
