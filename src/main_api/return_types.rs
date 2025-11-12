use leptos::prelude::ServerFnError;
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumString};
use uuid::Uuid;

use crate::event_sourcing::{
    journal::JournalTenantInfo,
    journal::{BalanceUpdate, Permissions, Transaction},
};

#[derive(EnumString, Display)]
pub enum KnownErrors {
    SessionIdNotFound,
    UsernameNotFound {
        username: String,
    },
    LoginFailed {
        username: String,
        password: String,
    },
    SignupPasswordMismatch {
        username: String,
    },

    UserDoesntExist,

    UserExists {
        username: String,
    },
    BalanceMismatch {
        attempted_transaction: Vec<BalanceUpdate>,
    },

    PermissionError {
        required_permissions: Permissions,
    },

    InvalidInput,

    NoInvitation,

    NotLoggedIn,
}

impl KnownErrors {
    pub fn parse_error(error: ServerFnError) -> Result<Self, String> {
        match error
            .to_string()
            .trim_start_matches("error running server function: ")
            .parse::<KnownErrors>()
        {
            Ok(s) => Ok(s),
            Err(_) => Err(format!(
                "An unexpected error occured: {}",
                error
                    .to_string()
                    .trim_start_matches("error running server function: ")
            )),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Account {
    pub name: String,
    pub balance: i64,
}

#[derive(Serialize, Deserialize)]
pub enum AssociatedJournal {
    Owned {
        id: Uuid,
        name: String,
    },
    Shared {
        id: Uuid,
        name: String,
        tenant_info: JournalTenantInfo,
    },
}

#[derive(Serialize, Deserialize)]
pub struct TransactionWithTimeStamp {
    pub transaction: Transaction,
    pub timestamp: i64,
}
