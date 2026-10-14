pub mod account;
pub mod activity;
pub mod commands;
pub mod entry;
pub mod error;
pub mod event;
#[expect(unused)]
pub mod event_store_revised;
#[expect(unused)]
mod file;
pub mod fund;
pub mod jewel;
pub mod layout;
pub mod member;
pub mod person;
pub mod service;
pub mod store;
pub mod transaction;
pub mod views;

use crate::id::Ident;
use crate::journal::event::{JournalDomainEvent, JournalEvent};
pub use service::JournalService;
use std::cmp::PartialEq;

use axum::Router;
use axum::routing::{get, post, put};
use axum_login::login_required;

id!(JournalId, Ident::new16());

pub fn router() -> Router<crate::StateType> {
    Router::new()
        .route("/journal", get(views::journal_list))
        .route("/createjournal", post(commands::create_journal))
        .route("/journal/{id}", get(views::journal_detail))
        .route("/journal/{id}/file", get(file::views::file_list_page))
        .route(
            "/journal/{id}/file/upload",
            post(file::commands::upload_file),
        )
        .route(
            "/journal/{id}/file/localupload",
            put(file::commands::localstore_file_handler)
                .layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route(
            "/journal/{id}/file/recordupload",
            post(file::commands::record_file_upload),
        )
        .route(
            "/journal/{id}/file/{file_id}",
            get(file::views::download_file),
        )
        .route("/journal/{id}/file/{file_id}/jewel", get(jewel::view_db))
        .route(
            "/journal/{id}/file/{file_id}/import_jewel",
            get(jewel::commands::import_jewel_db),
        )
        .route(
            "/journal/{journal_id}/file/{file_id}/delete",
            post(delete_file),
        )
        .route("/journal/{id}/person", get(person::people_list_page))
        .route("/journal/{id}/invite", post(commands::invite_member))
        .route(
            "/journal/{id}/person/{person_id}",
            get(person::person_detail_page),
        )
        .route(
            "/journal/{id}/person/{person_id}/update",
            post(commands::update_permissions),
        )
        .route(
            "/journal/{id}/person/{person_id}/remove",
            post(commands::remove_member),
        )
        .route_layer(login_required!(crate::BackendType, login_url = "/signin"))
}

use crate::authn::UserId;
use crate::authority::{Actor, Authority};
use crate::id;
use crate::journal::error::JournalError::InvalidJournal;
use crate::journal::error::{JournalError, JournalResult};
use crate::journal::file::commands::delete_file;
use crate::journal::fund::FundId;
use crate::journal::member::JournalMember;
use crate::name::Name;
use crate::status::Status;
use crate::time::Timestamp;
use axum::extract::DefaultBodyLimit;
use bitflags::bitflags;
use disintegrate::{Decision, StateMutate, StateQuery};
use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, FromRow, Postgres, Type};
use std::fmt::Display;
use std::fmt::Formatter;
use thiserror::Error;

/// validates that an `Authority` has sufficient permissions to perform an action
pub fn validate_permissions(
    member: &JournalMember,
    authority: Authority,
    journal_owner: UserId,
    permissions: Permissions,
) -> bool {
    if let Some(user_id) = authority.user_id()
        && user_id == journal_owner
    {
        return true;
    }

    if member.status.valid() && member.permissions.contains(permissions) {
        return true;
    }

    if matches!(authority.actor(), Actor::System) {
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

        Ok(vec![
            JournalDomainEvent::JournalCreated {
                journal_id: self.journal_id,
                owner: self.owner,
                name: self.name.clone(),
                authority: self.authority,
                timestamp: self.timestamp,
            },
            // seed the general fund when the journal is created
            JournalDomainEvent::FundCreated {
                fund_id: FundId::new(),
                journal_id: self.journal_id,
                fund_name: Name::try_new("General".to_string()).expect("valid fund name"),
                authority: Authority::Direct(Actor::System),
                timestamp: self.timestamp,
            },
        ])
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
            authority: self.authority,
            timestamp: self.timestamp,
        }])
    }
}

bitflags! {
    #[derive(Hash, Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Permissions: i32 {
        const READ = 1 << 0;
        const ADD_ACCOUNT = 1 << 1;
        const CREATE_ACTIVITY = 1 << 2;
        const CREATE_FUND = 1 << 3;
        const UPLOAD_FILE = 1 << 4;
        const APPEND_TRANSACTION = 1 << 5;
        const INVITE = 1 << 6;
        const OWNER = 1 << 7;
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
pub struct PermissionDecodeError(pub i32);

impl Display for PermissionDecodeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "an unknown bit was set in the permission value: {}",
            self.0
        )
    }
}

pub struct JournalState {
    pub id: JournalId,
    pub owner_id: UserId,
    pub name: Name,
}

#[derive(FromRow)]
pub struct JournalStateWithPayload {
    pub id: JournalId,
    pub owner_id: UserId,
    pub name: Name,
    pub payload: JournalDomainEvent,
}
