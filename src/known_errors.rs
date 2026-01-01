use crate::journal::{BalanceUpdate, Permissions};
use axum::response::{Html, IntoResponse, Response};
use leptos::prelude::{ElementChild, RenderHtml, ServerFnError, view};
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

pub fn return_error(e: ServerFnError, context: &str) -> Response {
    Html(
        view! { <p>"An error occurred while " {context.to_string()} ": " {e.to_string()}</p> }
            .to_html(),
    )
    .into_response()
}

pub fn error_message(message: &str) -> Response {
    Html(view! { <p>{message.to_string()}</p> }.to_html()).into_response()
}
