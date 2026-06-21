use crate::account::AccountId;
use crate::auth::user::UserId;
use crate::email::Email;
use crate::ident::Ident;
use crate::journal::{JournalId, Permissions};
use crate::store::universal::EventId;
use crate::store::universal::registry::EntityType;
use axum_test::expect_json::__private::serde_trampoline::Deserialize;
use deadpool_diesel::{InteractError, PoolError};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::watch::error::RecvError;

#[derive(Debug, Error, Clone, Deserialize, PartialEq)]
pub enum StoreError {
    #[error("failed to deserialize an event payload")]
    Deserialize(#[from] postcard::Error),

    #[error("sequence error: expected a maximum event id of {expected:?}, found {found:?}")]
    EventIdViolation { expected: EventId, found: EventId },

    #[error("incorrect entity type: expected {expected:?}, found {found:?}")]
    EntityType {
        expected: EntityType,
        found: EntityType,
    },

    #[error("attempted to apply an update to the transaction {0}, but it doesn't exist")]
    TransactionModifiedBeforeCreation(Ident),

    #[error("attempted to apply an update to the transaction {0}, but it was deleted")]
    TransactionModifiedAfterDeletion(Ident),

    #[error("attempted to apply an update to the account {0}, but it doesn't exist")]
    AccountModifiedBeforeCreation(Ident),

    #[error("deadpool_diesel returned an error: {0}")]
    Pool(String),

    #[error("a diesel query returned an error: {0}")]
    Query(String),

    #[error("a deadpool_diesel interaction returned an error")]
    Interact(String),

    #[error("failed to send a value through a tokio channel")]
    Send(String),

    #[error("failed to recieve a value through a tokio channel")]
    Receive(String),

    #[error("the actor does not have the permissions required to access this resource: {0:?}")]
    Permission(Permissions),

    #[error("the queried entity does not exist or is not accessible")]
    EntityNotFound,

    #[error("account {account_id} is not accessible in journal {journal_id}")]
    AccountNotInJournal {
        journal_id: JournalId,
        account_id: AccountId,
    },

    #[error("the invited user already has access to the journal")]
    JournalInviteUserHasAccess(Email),

    #[error("the target user does not have journal access")]
    JournalModifyNoAccess(UserId),
}

impl From<PoolError> for StoreError {
    fn from(value: PoolError) -> Self {
        Self::Pool(value.to_string())
    }
}

impl From<diesel::result::Error> for StoreError {
    fn from(value: diesel::result::Error) -> Self {
        if matches!(value, diesel::result::Error::NotFound) {
            Self::EntityNotFound
        } else {
            Self::Query(value.to_string())
        }
    }
}

impl From<InteractError> for StoreError {
    fn from(value: InteractError) -> Self {
        Self::Interact(value.to_string())
    }
}

impl<T> From<SendError<T>> for StoreError {
    fn from(value: SendError<T>) -> Self {
        Self::Send(value.to_string())
    }
}

impl From<RecvError> for StoreError {
    fn from(value: RecvError) -> Self {
        Self::Receive(value.to_string())
    }
}

pub type StoreResult<T> = Result<T, StoreError>;
