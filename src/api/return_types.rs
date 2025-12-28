use std::fmt;

use chrono::Utc;
use cuid::{Cuid2Constructor, cuid2_slug, is_cuid2};
use leptos::prelude::ServerFnError;
use serde::{Deserialize, Serialize};

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

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cuid {
    Cuid10([u8; 10]),
    Cuid16([u8; 16]),
}

impl Cuid {
    pub fn new10() -> Self {
        Self::Cuid10(
            cuid2_slug()
                .as_bytes()
                .try_into()
                .expect("failed to generate new cuid10"),
        )
    }
    pub fn new16() -> Self {
        Self::Cuid16(
            Cuid2Constructor::new()
                .with_length(16)
                .create_id()
                .as_bytes()
                .try_into()
                .expect("failed to generate new cuid16"),
        )
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ServerFnError> {
        let str = str::from_utf8(bytes)?;
        Self::from_str(str)
    }

    pub fn from_str(str: &str) -> Result<Self, ServerFnError> {
        if !is_cuid2(str) {
            return Err(ServerFnError::ServerError(
                KnownErrors::InvalidId.to_string()?,
            ));
        }
        match str.len() {
            10 => Ok(Self::Cuid10(str.as_bytes().try_into()?)),
            16 => Ok(Self::Cuid16(str.as_bytes().try_into()?)),
            _ => Err(ServerFnError::ServerError(
                KnownErrors::InvalidId.to_string()?,
            )),
        }
    }

    pub fn to_bytes(self) -> Vec<u8> {
        match self {
            Cuid::Cuid10(id) => Vec::from(id),
            Cuid::Cuid16(id) => Vec::from(id),
        }
    }
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for Cuid {
    fn default() -> Self {
        Self::from_str("aaaaaaaaaa").expect("failed to generate default Cuid")
    }
}

// this has the potential to panic if the id is created manually rather than with helper functions
impl fmt::Display for Cuid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Cuid::Cuid10(id) => write!(
                f,
                "{}",
                str::from_utf8(id).expect("failed to convert Cuid10 to string")
            ),
            Cuid::Cuid16(id) => write!(
                f,
                "{}",
                str::from_utf8(id).expect("failed to convert Cuid16 to string")
            ),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Account {
    pub id: Cuid,
    pub name: String,
    pub balance: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum AssociatedJournal {
    Owned {
        id: Cuid,
        name: String,
        created_at: chrono::DateTime<Utc>,
    },
    Shared {
        id: Cuid,
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
    pub fn get_id(&self) -> Cuid {
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
    pub id: Cuid,
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
