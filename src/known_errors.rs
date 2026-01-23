use core::fmt;
use std::{array::TryFromSliceError, num::ParseIntError, str::Utf8Error};

use crate::{
    ident::AccountId,
    journal::{Permissions, transaction::BalanceUpdate},
    webauthn::user::Email,
};
use axum::response::Redirect;
use base64::{Engine, engine::general_purpose};
use postcard::to_allocvec;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub enum KnownErrors {
    InternalError {
        context: String,
    },

    DatabaseError {
        context: String,
    },

    PostcardError {
        context: Box<postcard::Error>,
    },

    SessionIdNotFound,

    UsernameNotFound {
        username: String,
    },

    InvalidEmail {
        email: String,
    },

    LoginFailed {
        email: Email,
    },

    SignupPasswordMismatch {
        email: Email,
    },

    UserDoesntExist,

    AccountDoesntExist {
        id: AccountId,
    },

    UserExists {
        email: Email,
    },

    AccountExists,

    BalanceMismatch {
        attempted_transaction: Vec<BalanceUpdate>,
    },

    PermissionError {
        required_permissions: Permissions,
    },

    TenantDoesntExist,

    InvalidInput,

    TransactionDoesntExist,

    InvalidId,

    NoInvitation,

    NotLoggedIn,

    UserCanAccessJournal,

    InvalidJournal,

    IncorrectEventType,

    OsError {
        context: String,
    },

    ParseInt {
        err: String,
    },

    None,
}

pub type MonkestoResult<T> = Result<T, KnownErrors>;

impl KnownErrors {
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
            .unwrap_or(Self::None)
    }
}

impl fmt::Display for KnownErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for KnownErrors {}

impl From<tokio::task::JoinError> for KnownErrors {
    fn from(err: tokio::task::JoinError) -> Self {
        Self::InternalError {
            context: err.to_string(),
        }
    }
}

impl From<sqlx::Error> for KnownErrors {
    fn from(err: sqlx::Error) -> Self {
        Self::DatabaseError {
            context: err.to_string(),
        }
    }
}

impl From<postcard::Error> for KnownErrors {
    fn from(err: postcard::Error) -> Self {
        Self::PostcardError {
            context: Box::new(err),
        }
    }
}

impl From<base64::DecodeError> for KnownErrors {
    fn from(err: base64::DecodeError) -> Self {
        Self::InternalError {
            context: err.to_string(),
        }
    }
}

impl From<Utf8Error> for KnownErrors {
    fn from(err: Utf8Error) -> Self {
        Self::InternalError {
            context: err.to_string(),
        }
    }
}

impl From<TryFromSliceError> for KnownErrors {
    fn from(err: TryFromSliceError) -> Self {
        Self::InternalError {
            context: err.to_string(),
        }
    }
}

impl From<ParseIntError> for KnownErrors {
    fn from(value: ParseIntError) -> Self {
        Self::ParseInt {
            err: value.to_string(),
        }
    }
}

pub trait RedirectOnError<T> {
    /// redirects to the given page, passing E
    fn or_redirect(self, page: &str) -> Result<T, Redirect>;

    /// redirects to the given page without passing E
    #[expect(dead_code)]
    fn or_redirect_clean(self, page: &str) -> Result<T, Redirect>;

    #[expect(dead_code)]
    /// redirects to the given page, passing the given err
    fn or_redirect_override(self, err: KnownErrors, page: &str) -> Result<T, Redirect>;
}

impl<T, E> RedirectOnError<T> for Result<T, E>
where
    E: Into<KnownErrors>,
{
    fn or_redirect(self, page: &str) -> Result<T, Redirect> {
        self.map_err(|e| e.into().redirect(page))
    }
    fn or_redirect_clean(self, page: &str) -> Result<T, Redirect> {
        self.map_err(|_| Redirect::to(page))
    }
    fn or_redirect_override(self, err: KnownErrors, page: &str) -> Result<T, Redirect> {
        self.map_err(|_| err.redirect(page))
    }
}

#[derive(Deserialize)]
pub struct UrlError {
    pub err: Option<String>,
    #[expect(dead_code)]
    pub next: Option<String>,
}
