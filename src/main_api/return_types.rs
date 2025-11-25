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
    None,

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

    InvalidJournal,
}

impl KnownErrors {
    pub fn parse_error(error: ServerFnError) -> Option<Self> {
        error
            .to_string()
            .trim_start_matches("error running server function: ")
            .parse::<KnownErrors>()
            .ok()
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Account {
    pub name: String,
    pub balance: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
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

impl AssociatedJournal {
    pub fn get_id(&self) -> Uuid {
        match self {
            Self::Owned { id, .. } => *id,
            Self::Shared { id, .. } => *id,
        }
    }
    pub fn get_name(&self) -> String {
        match self {
            Self::Owned { name, .. } => name.clone(),
            Self::Shared { name, .. } => name.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Journals {
    pub associated: Vec<AssociatedJournal>,
    pub selected: Option<AssociatedJournal>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TransactionWithTimeStamp {
    pub transaction: Transaction,
    pub timestamp: i64,
}
