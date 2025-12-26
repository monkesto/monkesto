use chrono::Utc;
use leptos::prelude::ServerFnError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::event_sourcing::{
    journal::JournalTenantInfo,
    journal::{BalanceUpdate, Permissions},
};

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

    NoInvitation,

    NotLoggedIn,

    UserCanAccessJournal,

    InvalidJournal,
}

impl KnownErrors {
    pub fn to_string(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn parse_error(error: ServerFnError) -> Option<Self> {
        serde_json::from_str(
            error
                .to_string()
                .trim_start_matches("error running server function: "),
        )
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
        created_at: chrono::DateTime<Utc>,
    },
    Shared {
        id: Uuid,
        name: String,
        created_at: chrono::DateTime<Utc>,
        tenant_info: JournalTenantInfo,
    },
}

impl AssociatedJournal {
    fn has_permission(&self, permissions: Permissions) -> bool {
        match self {
            Self::Owned { .. } => true,
            Self::Shared { tenant_info, .. } => {
                tenant_info.tenant_permissions.contains(permissions)
            }
        }
    }
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
    pub fn get_created_at(&self) -> chrono::DateTime<Utc> {
        match self {
            Self::Owned { created_at, .. } => *created_at,
            Self::Shared { created_at, .. } => *created_at,
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Journals {
    pub associated: Vec<AssociatedJournal>,
    pub selected: Option<AssociatedJournal>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct JournalInvite {
    pub id: Uuid,
    pub name: String,
    pub tenant_info: JournalTenantInfo,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TransactionWithUsername {
    pub author: String,
    pub updates: Vec<BalanceUpdate>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TransactionWithTimeStamp {
    pub transaction: TransactionWithUsername,
    pub timestamp: chrono::DateTime<Utc>,
}
