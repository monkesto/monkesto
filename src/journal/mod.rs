pub mod account;
pub mod activity;
pub mod commands;
pub mod domain;
pub mod entry;
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
pub use service::JournalService;
use std::cmp::PartialEq;

use axum::Router;
use axum::routing::{get, post, put};
use axum_login::login_required;

id!(JournalId, Ident::new16());

#[derive(Error, Debug, PartialEq)]
pub enum JournalError {
    #[error("a journal already exists with the id {0}")]
    IdCollision(JournalId),

    #[error("an account already exists with the id {0}")]
    AccountIdCollision(AccountId),

    #[error("an activity already exists with the id {0}")]
    ActivityIdCollision(ActivityId),

    #[error("a fund already exists with the id {0}")]
    FundIdCollision(FundId),

    #[error("an entry already exists with the id {0}")]
    EntryIdCollision(EntryId),

    #[error("a transaction already exists with the id {0}")]
    TransactionIdCollision(TransactionId),

    #[error("a file already exists with the id {0}")]
    FileIdCollision(FileId),

    #[error("invalid journal: {0}")]
    InvalidJournal(JournalId),

    #[error("invalid account: {0}")]
    InvalidAccount(AccountId),

    #[error("invalid activity: {0}")]
    InvalidActivity(ActivityId),

    #[error("invalid fund: {0}")]
    InvalidFund(FundId),

    #[error("invalid entry: {0}")]
    InvalidEntry(EntryId),

    #[error("invalid transaction: {0}")]
    InvalidTransaction(TransactionId),

    #[error("failed to validate a transaction: {0}")]
    TransactionValidation(#[from] TransactionValidationError),

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

    #[error("failed to construct permissions from an integer: {0}")]
    PermissionDecode(#[from] PermissionDecodeError),

    #[error("failed to decode an event: {0}")]
    EventDecode(String),

    #[error("failed to decode a proto type: {0}")]
    ProtoDecode(#[from] ProtoError),

    #[error("the server-side S3 credentials are invalid")]
    InvalidS3Credentials,

    #[error("an S3 transaction failed: {0}")]
    S3(String),

    #[error("invalid file: {0}")]
    InvalidFile(FileId),

    #[error("failed to import from a jewel database: {0}")]
    JewelImport(#[from] JewelImportError),
}

impl From<sqlx::Error> for JournalError {
    fn from(value: Error) -> Self {
        Self::Sqlx(value.to_string())
    }
}

impl From<prost::DecodeError> for JournalError {
    fn from(value: prost::DecodeError) -> Self {
        Self::EventDecode(value.to_string())
    }
}

impl<E, R> From<SdkError<E, R>> for JournalError {
    fn from(value: SdkError<E, R>) -> Self {
        Self::S3(value.to_string())
    }
}

pub type JournalResult<T> = Result<T, JournalError>;

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
            put(file::commands::localstore_file_handler).layer(DefaultBodyLimit::disable()),
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

use crate::authn::user::UserId;
use crate::authority::{Actor, Authority};
use crate::event_id::GetEventId;
use crate::id;
use crate::id::IdentError;
use crate::journal::JournalError::InvalidJournal;
use crate::journal::account::AccountId;
use crate::journal::activity::ActivityId;
use crate::journal::domain::JournalDomainEvent;
use crate::journal::entry::EntryId;
use crate::journal::file::FileId;
use crate::journal::fund::FundId;
use crate::journal::jewel::JewelImportError;
use crate::journal::member::{
    AddJournalMember, JournalMember, RemoveJournalMember, UpdateJournalMember,
};
use crate::journal::transaction::{TransactionId, TransactionValidationError};
use crate::name::Name;
use crate::serde::error::ProtoError;
use crate::status::Status;
use crate::time_provider::Timestamp;
use aws_sdk_s3::error::SdkError;
use axum::extract::DefaultBodyLimit;
use bitflags::bitflags;
use disintegrate::{Decision, DecisionError, StateMutate, StateQuery};
use disintegrate_postgres::PgEventId;
use domain::JournalEvent;
use prost::Message;
use proto::event::journal::ProtoJournalDomainEvent;
use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Error, FromRow, Postgres, Type};
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
struct JournalStateWithPayload {
    id: JournalId,
    owner_id: UserId,
    name: Name,
    payload: Vec<u8>,
}

impl JournalService {
    pub async fn create_journal(
        &self,
        journal_id: JournalId,
        owner: UserId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(CreateJournal::new(
                journal_id, owner, name, authority, timestamp,
            ))
            .await?
            .event_id())
    }

    pub async fn add_member(
        &self,
        journal_id: JournalId,
        member_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(AddJournalMember::new(
                journal_id,
                member_id,
                permissions,
                authority,
                timestamp,
            ))
            .await?
            .event_id())
    }

    pub async fn update_member(
        &self,
        journal_id: JournalId,
        member_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(UpdateJournalMember::new(
                journal_id,
                member_id,
                permissions,
                authority,
                timestamp,
            ))
            .await?
            .event_id())
    }

    pub async fn remove_member(
        &self,
        journal_id: JournalId,
        member_id: UserId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(RemoveJournalMember::new(
                journal_id, member_id, authority, timestamp,
            ))
            .await?
            .event_id())
    }

    pub async fn get_journal(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<(JournalState, Authority, Timestamp)> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(JournalError::InvalidJournal(journal_id));
        }

        let journal = sqlx::query_as!(
            JournalStateWithPayload,
            r#"
            SELECT j.id as "id: JournalId", j.owner_id as "owner_id: UserId", j.name as "name: Name", e.payload as "payload!"
            FROM journals j
            INNER JOIN event e
                ON e.journal_id = $1 AND e.event_type = 'JournalCreated'
            WHERE j.id = $1
            "#,
            journal_id as JournalId)
            .fetch_optional(&self.projection_pool)
            .await?;

        if let Some(journal) = journal {
            let payload = JournalDomainEvent::try_from(ProtoJournalDomainEvent::decode(
                journal.payload.as_slice(),
            )?)?;

            match payload {
                JournalDomainEvent::JournalCreated {
                    authority,
                    timestamp,
                    ..
                } => Ok((
                    JournalState {
                        id: journal.id,
                        owner_id: journal.owner_id,
                        name: journal.name,
                    },
                    authority,
                    timestamp,
                )),
                _ => unreachable!("JournalCreated events are filtered by the sql query"),
            }
        } else {
            Err(InvalidJournal(journal_id))
        }
    }

    /// returns the current state, creation authority, and creation timestamp of every accessible journal
    pub async fn list_accessible_journals(
        &self,
        user: UserId,
    ) -> JournalResult<Vec<(JournalState, Authority, Timestamp)>> {
        // NOTE(gabriel): a user must not be both a member and the owner, or this query will return duplicate journals

        let journals = sqlx::query_as!(
            JournalStateWithPayload,
            r#"
            SELECT j.id as "id: JournalId", j.owner_id as "owner_id: UserId", j.name as "name: Name", e.payload as "payload!"
            FROM journals j
            INNER JOIN event e
                ON e.journal_id = j.id AND e.event_type = 'JournalCreated'
            LEFT JOIN journal_members jm ON jm.journal_id = j.id AND (jm.permissions & $1) = $1
            WHERE j.owner_id = $2 OR jm.user_id = $2
            "#,
            Permissions::READ.bits(),
            user as UserId)
            .fetch_all(&self.projection_pool)
            .await?;

        // TODO(gabriel) would .map() be more efficient here?
        let mut journals_with_meta = Vec::with_capacity(journals.len());

        for journal in journals {
            let payload = JournalDomainEvent::try_from(ProtoJournalDomainEvent::decode(
                journal.payload.as_slice(),
            )?)?;

            match payload {
                JournalDomainEvent::JournalCreated {
                    authority,
                    timestamp,
                    ..
                } => {
                    journals_with_meta.push((
                        JournalState {
                            id: journal.id,
                            owner_id: journal.owner_id,
                            name: journal.name,
                        },
                        authority,
                        timestamp,
                    ));
                }
                _ => unreachable!("JournalCreated events are filtered by the sql query"),
            }
        }

        Ok(journals_with_meta)
    }

    pub async fn list_journal_members(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<UserId>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(InvalidJournal(journal_id));
        }

        Ok(sqlx::query_scalar!(
            r#"
            SELECT user_id as "user_id: UserId" FROM journal_members WHERE journal_id = $1
            "#,
            journal_id as JournalId
        )
        .fetch_all(&self.projection_pool)
        .await?)
    }
}
