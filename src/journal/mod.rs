pub mod account;
pub mod commands;
pub mod domain;
pub mod layout;
pub mod member;
pub mod person;
pub mod service;
pub mod store;
pub mod transaction;
pub mod views;

use crate::id::Ident;
pub use service::JournalService;
use std::cmp::PartialEq;

use axum::Router;
use axum::routing::get;
use axum_login::login_required;

id!(JournalId, Ident::new16());

#[derive(Error, Debug, Serialize, Deserialize, PartialEq)]
pub enum JournalError {
    #[error("a journal already exists with the id {0}")]
    IdCollision(JournalId),

    #[error("an account already exists with the id {0}")]
    AccountIdCollision(AccountId),

    #[error("a transaction already exists with the id {0}")]
    TransactionIdCollision(TransactionId),

    #[error("invalid journal: {0}")]
    InvalidJournal(JournalId),

    #[error("invalid account: {0}")]
    InvalidAccount(AccountId),

    #[error("invalid transaction: {0}")]
    InvalidTransaction(TransactionId),

    #[error("failed to validate a transaction: {0}")]
    TransactionValidation(#[from] TransactionValidationError),

    #[error("user doesn't exist: {0}")]
    InvalidUser(UserId),

    #[error("The user doesn't have the {:?} permission", .0)]
    Permissions(Permissions),

    #[error("The user {0} already has access to this journal")]
    UserAlreadyHasAccess(UserId),

    #[error("The user {0} doesn't have access to this journal")]
    UserDoesntHaveAccess(UserId),

    #[error("Failed to create an Ident: {0}")]
    IdentCreation(#[from] IdentError),

    #[error("sqlx returned an error: {0}")]
    Sqlx(String),

    #[error("failed to serialize or deserialize a value with rmp-serde: {0}")]
    MsgPack(String),

    #[error("failed to construct permissions from an integer: {0}")]
    PermissionDecode(#[from] PermissionDecodeError),
}

impl From<sqlx::Error> for JournalError {
    fn from(value: Error) -> Self {
        Self::Sqlx(value.to_string())
    }
}

impl From<rmp_serde::decode::Error> for JournalError {
    fn from(value: rmp_serde::decode::Error) -> Self {
        Self::MsgPack(value.to_string())
    }
}

pub type JournalResult<T> = Result<T, JournalError>;

pub fn router() -> Router<crate::StateType> {
    Router::new()
        .route("/journal", get(views::journal_list))
        .route(
            "/createjournal",
            axum::routing::post(commands::create_journal),
        )
        .route("/journal/{id}", get(views::journal_detail))
        .route("/journal/{id}/person", get(person::people_list_page))
        .route(
            "/journal/{id}/invite",
            axum::routing::post(commands::invite_member),
        )
        .route(
            "/journal/{id}/person/{person_id}",
            get(person::person_detail_page),
        )
        .route(
            "/journal/{id}/person/{person_id}/update",
            axum::routing::post(commands::update_permissions),
        )
        .route(
            "/journal/{id}/person/{person_id}/remove",
            axum::routing::post(commands::remove_member),
        )
        .route_layer(login_required!(crate::BackendType, login_url = "/signin"))
}

use crate::authn::user::UserId;
use crate::authority::{Actor, Authority};
use crate::id;
use crate::id::IdentError;
use crate::journal::JournalError::InvalidJournal;
use crate::journal::account::AccountId;
use crate::journal::domain::JournalDomainEvent;
use crate::journal::member::JournalMember;
use crate::journal::transaction::{TransactionId, TransactionValidationError};
use crate::name::Name;
use crate::status::Status;
use crate::time_provider::Timestamp;
use bitflags::bitflags;
use disintegrate::{Decision, StateMutate, StateQuery};
use domain::JournalEvent;
use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Error, Postgres, Type};
use std::fmt::Display;
use std::fmt::Formatter;
use thiserror::Error;

/// validates that an `Authority` has sufficient permissions to perform an action
pub fn validate_permissions(
    member: &JournalMember,
    authority: &Authority,
    journal_owner: UserId,
    permissions: Permissions,
) -> bool {
    if let Some(user_id) = authority.user_id()
        && user_id == journal_owner
    {
        return true;
    }

    if (member.status.valid() && member.permissions.contains(permissions))
        || matches!(authority.actor(), Actor::System)
    {
        return true;
    }

    false
}

#[derive(StateQuery, Clone, Default, Serialize, Deserialize)]
#[state_query(JournalEvent)]
pub struct Journal {
    #[id]
    pub journal_id: JournalId,
    pub owner: UserId,
    pub name: Name,
    pub status: Status,
}

impl Journal {
    pub fn new(journal_id: JournalId) -> Self {
        Self {
            journal_id,
            ..Default::default()
        }
    }
}

impl StateMutate for Journal {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            JournalEvent::JournalCreated { owner, name, .. } => {
                self.owner = owner;
                self.name = name;
                self.status = Status::Valid;
            }
            JournalEvent::JournalDeleted { .. } => self.status = Status::Deleted,
        }
    }
}

pub struct CreateJournal {
    journal_id: JournalId,
    owner: UserId,
    name: Name,
    authority: Authority,
    timestamp: Timestamp,
}

impl CreateJournal {
    pub fn new(
        journal_id: JournalId,
        owner: UserId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            journal_id,
            owner,
            name,
            authority,
            timestamp,
        }
    }
}

impl Decision for CreateJournal {
    type Event = JournalDomainEvent;
    type StateQuery = Journal;
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        Journal::new(self.journal_id)
    }

    fn process(&self, state: &Self::StateQuery) -> Result<Vec<Self::Event>, Self::Error> {
        if state.status.found() {
            return Err(JournalError::IdCollision(self.journal_id));
        }

        Ok(vec![JournalDomainEvent::JournalCreated {
            journal_id: self.journal_id,
            owner: self.owner,
            name: self.name.clone(),
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

pub struct DeleteJournal {
    journal_id: JournalId,
    authority: Authority,
    timestamp: Timestamp,
}

#[expect(unused)]
impl DeleteJournal {
    pub fn new(journal_id: JournalId, authority: Authority, timestamp: Timestamp) -> Self {
        Self {
            journal_id,
            authority,
            timestamp,
        }
    }
}

impl Decision for DeleteJournal {
    type Event = JournalDomainEvent;
    type StateQuery = Journal;
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        Journal::new(self.journal_id)
    }

    fn process(&self, state: &Self::StateQuery) -> Result<Vec<Self::Event>, Self::Error> {
        if !state.status.valid() {
            return Err(InvalidJournal(state.journal_id));
        }

        Ok(vec![JournalDomainEvent::JournalDeleted {
            journal_id: self.journal_id,
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

bitflags! {
    #[derive(Hash, Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Permissions: i32 {
        const READ = 1 << 0;
        const ADD_ACCOUNT = 1 << 1;
        const APPEND_TRANSACTION = 1 << 2;
        const INVITE = 1 << 3;
        const OWNER = 1 << 4;
    }
}

impl Type<Postgres> for Permissions {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <i32 as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for Permissions {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <i32 as Encode<Postgres>>::encode(self.bits(), buf)
    }
}

impl<'r> Decode<'r, Postgres> for Permissions {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let val = <i32 as Decode<Postgres>>::decode(value)?;
        Ok(Permissions::from_bits(val).ok_or(PermissionDecodeError(val))?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Error, PartialEq)]
pub struct PermissionDecodeError(i32);

impl Display for PermissionDecodeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "an unknown bit was set in the permission value: {}",
            self.0
        )
    }
}
