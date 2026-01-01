use std::fmt::Display;

use crate::journal::{BalanceUpdate, Permissions};
use axum::response::{IntoResponse, Response};
use leptos::prelude::ServerFnError;
use maud::html;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq)]
pub enum KnownErrors {
    None,

    SessionIdNotFound,

    UsernameNotFound {
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
}

impl KnownErrors {
    pub fn to_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn parse_error(error: &ServerFnError) -> Option<Self> {
        serde_json::from_str(
            error
                .to_string()
                .trim_start_matches("error running server function: "),
        )
        .ok()
    }
}

pub fn return_error(e: impl Display, context: &str) -> Response {
    crate::maud_header::header(html! {
        p {
            "An error occurred while " (context) ": " (e)
        }
    })
    .into_response()
}

pub fn error_message(message: &str) -> Response {
    crate::maud_header::header(html! {
        p {
            (message)
        }
    })
    .into_response()
}

#[macro_export]
macro_rules! ok_or_return_error {
    ($result: expr, $context: expr) => {
        match $result {
            Ok(s) => s,
            Err(e) => return return_error(e, $context),
        }
    };
}
