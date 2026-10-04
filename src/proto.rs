#![allow(clippy::module_inception)]

use crate::email::EmailError;
use crate::id::ident_error::IdentError;
use crate::journal::account::AccountTypeFromIntError;
use crate::journal::activity::kind::ActivityKindFromIntError;
use crate::journal::activity::scope::ActivityScopeFromIntError;
use crate::journal::entry::EntrySideFromIntError;
use crate::journal::transaction::FinancialPeriodFromIntError;
use crate::journal::transaction::memo::MemoError;
use crate::name::NameError;
use thiserror::Error;

pub mod name {
    pub mod name {
        include!(concat!(env!("OUT_DIR"), "/proto.name.name.rs"));
    }
}

pub mod journal {
    pub mod event {
        pub mod journal_event {
            include!(concat!(env!("OUT_DIR"), "/proto.journal.event.journal.rs"));
        }
    }

    pub mod entry {
        pub mod entry {
            include!(concat!(env!("OUT_DIR"), "/proto.journal.entry.entry.rs"));
        }
    }

    pub mod transaction {
        pub mod memo {
            pub mod memo {
                include!(concat!(
                    env!("OUT_DIR"),
                    "/proto.journal.transaction.memo.memo.rs"
                ));
            }
        }
    }
}

pub mod authn {
    pub mod event {
        pub mod authn {
            include!(concat!(env!("OUT_DIR"), "/proto.authn.event.authn.rs"));
        }
    }
}

pub mod id {
    pub mod ident {
        include!(concat!(env!("OUT_DIR"), "/proto.id.ident.rs"));
    }
}

pub mod authz {
    pub mod event {
        pub mod authz {
            include!(concat!(env!("OUT_DIR"), "/proto.authz.event.authz.rs"));
        }
    }
}

pub mod time {
    pub mod timestamp {
        include!(concat!(env!("OUT_DIR"), "/proto.time.timestamp.rs"));
    }
}

pub mod authority {
    pub mod authority {
        include!(concat!(env!("OUT_DIR"), "/proto.authority.authority.rs"));
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum DecodeError {
    #[error("failed to deserialize the error")]
    Deserialize,
    #[error("expected a field that was missing")]
    FieldRequired,
    #[error("Failed to decode permissions from bits: {0}")]
    PermissionDecode(i32),
    #[error("Failed to parse an email: {0}")]
    ParseEmail(#[from] EmailError),
    #[error("Failed to parse a name: {0}")]
    ParseName(#[from] NameError),
    #[error("Failed to parse a memo: {0}")]
    ParseMemo(#[from] MemoError),
    #[error("Invalid ident: {0}")]
    Ident(#[from] IdentError),
    #[error("invalid webauthn-uuid: {0}")]
    ParseUuid(String),
    #[error("failed to parse an AccountType from the int {0}")]
    AccountTypeFromInt(#[from] AccountTypeFromIntError),
    #[error("failed to parse a FinancialPeriod from the int {0}")]
    FinancialPeriodFromInt(#[from] FinancialPeriodFromIntError),
    #[error("failed to parse an EntrySide from the int {0}")]
    EntrySideFromInt(#[from] EntrySideFromIntError),
    #[error("failed to parse an ActivityType from the int {0}")]
    ActivityTypeFromInt(#[from] ActivityKindFromIntError),
    #[error("failed to parse an ActivityScope from the int {0}")]
    ActivityScopeFromInt(#[from] ActivityScopeFromIntError),
    #[error("invalid prost message: {0}")]
    InvalidMessage(#[from] prost::DecodeError),
}
