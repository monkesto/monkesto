use crate::account::AccountPayload;
use crate::auth::passkey::PasskeyPayload;
use crate::auth::user::UserPayload;
use crate::authority::Authority;
use crate::ident::Ident;
use crate::journal::JournalPayload;
use crate::postcard::Postcard;
use crate::store::universal::example_entity::ExamplePayload;
use crate::store::universal::registry::{AnyPayload, EntityType};
use crate::store::universal::time_provider::TimeProvider;
use crate::transaction::TransactionPayload;
use chrono::{DateTime, Utc};
use deadpool_diesel::{InteractError, PoolError};
use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::{BigInt, Binary};
use diesel::{
    AsExpression, FromSqlRow, Insertable, Queryable, QueryableByName, Selectable, SqliteConnection,
    deserialize, serialize,
};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::ops::{Add, Deref};
use thiserror::Error;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::watch::error::RecvError;
use tower_sessions::ExpiredDeletion;

mod diesel_sqlite;
mod example_entity;
pub mod registry;
mod time_provider;

#[derive(Debug, Error, Clone, Deserialize)]
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

    #[error("")]
    Receive(String),
}

impl From<PoolError> for StoreError {
    fn from(value: PoolError) -> Self {
        Self::Pool(value.to_string())
    }
}

impl From<diesel::result::Error> for StoreError {
    fn from(value: diesel::result::Error) -> Self {
        Self::Query(value.to_string())
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

pub trait EntityId:
    Sync
    + Send
    + Deref<Target = Ident>
    + From<Ident>
    + Serialize
    + Copy
    + diesel::expression::AsExpression<Binary>
    + 'static
{
    fn as_bytes(&self) -> &[u8];
}

pub trait Payload:
    Send
    + Sync
    + Clone
    + Serialize
    + DeserializeOwned
    + diesel::expression::AsExpression<Binary>
    + Into<AnyPayload>
{
    fn as_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("Failed to serialize payload")
    }
    fn from_bytes(bytes: &[u8]) -> StoreResult<Self> {
        postcard::from_bytes(bytes)?
    }

    fn creates_entity(&self) -> bool;
}

#[allow(clippy::type_complexity)]
pub enum PayloadUsage<T: Entity> {
    CreatesState(T::State),
    ModifiesState(Box<dyn FnOnce(&mut T::State)>),
}

pub enum After {
    Start,
    Id(EventId),
}

/// A condition for recording an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    /// Record only if the stream is empty.
    Empty,
    /// Record only if the stream has no events beyond `T`.
    Within(EventId),
}

pub trait GetPayloadUsage<T: Entity> {
    fn usage<U: Into<T::Id>>(self, entity_id: U, event_id: EventId) -> PayloadUsage<T>;
}

pub trait Entity: Sized {
    type Id: EntityId;
    type Payload: Payload + GetPayloadUsage<Self>;
    type State: State + FetchState<Self> + 'static;

    fn entity_type() -> EntityType;
}

pub trait FetchState<I: Entity>: Sized {
    fn fetch(conn: &mut SqliteConnection, id: I::Id) -> StoreResult<Self>;
}

pub trait State: Send + Sync + Clone + Serialize + DeserializeOwned {
    fn as_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("Failed to serialize payload")
    }

    fn from_bytes(bytes: &[u8]) -> StoreResult<Self> {
        postcard::from_bytes(bytes)?
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    Eq,
    PartialOrd,
    Ord,
    Copy,
    AsExpression,
    FromSqlRow,
)]
#[diesel(sql_type = diesel::sql_types::BigInt)]
pub struct EventId(pub i64);

impl Add<i32> for EventId {
    type Output = EventId;

    fn add(self, rhs: i32) -> Self::Output {
        EventId(self.0 + rhs as i64)
    }
}

impl<DB: Backend> ToSql<BigInt, DB> for EventId
where
    i64: ToSql<BigInt, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, DB>) -> serialize::Result {
        self.0.to_sql(out)
    }
}

impl<DB: Backend> FromSql<BigInt, DB> for EventId
where
    i64: FromSql<BigInt, DB>,
{
    fn from_sql(value: DB::RawValue<'_>) -> deserialize::Result<Self> {
        i64::from_sql(value).map(EventId)
    }
}

impl Deref for EventId {
    type Target = i64;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Eq, AsExpression, FromSqlRow)]
#[diesel(sql_type = diesel::sql_types::BigInt)]
pub struct TimeStamp(DateTime<Utc>);

impl ToSql<BigInt, diesel::sqlite::Sqlite> for TimeStamp {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, diesel::sqlite::Sqlite>) -> serialize::Result {
        out.set_value(self.0.timestamp_millis());
        Ok(serialize::IsNull::No)
    }
}

impl ToSql<BigInt, diesel::pg::Pg> for TimeStamp {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, diesel::pg::Pg>) -> serialize::Result {
        let millis = self.0.timestamp_millis();
        <i64 as ToSql<BigInt, diesel::pg::Pg>>::to_sql(&millis, &mut out.reborrow())
    }
}

impl<DB: Backend> FromSql<BigInt, DB> for TimeStamp
where
    i64: FromSql<BigInt, DB>,
{
    fn from_sql(value: DB::RawValue<'_>) -> deserialize::Result<Self> {
        i64::from_sql(value).map(|val| {
            TimeStamp(DateTime::from_timestamp_millis(val).expect("failed to parse a timestamp"))
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Queryable, Selectable, Insertable, QueryableByName)]
#[diesel(table_name = crate::schema::events)]
pub struct Event<I: Entity> {
    pub event_id: EventId,
    pub timestamp: TimeStamp,
    pub authority: Postcard<Authority>,
    pub entity_id: I::Id,
    pub payload: I::Payload,
    pub applied_to_state: bool,
}

macro_rules! payload_from_bytes_match {
    ($bytes: ident, $entity_type: ident, $( $variant:path => $payload_type:ty),* $(,)?) => {
        match $entity_type{
                $(
                    $variant => Ok(postcard::from_bytes::<$payload_type>(
                        $bytes,
                    )?.into()),
                )*
                EntityType::Grant | EntityType::Role => todo!("grant and role entities do not have suitable payload types yet")
            }
    };
}

pub fn payload_from_bytes(bytes: &[u8], entity_type: EntityType) -> postcard::Result<AnyPayload> {
    payload_from_bytes_match! (
        bytes,
        entity_type,
        EntityType::Example => ExamplePayload,
        EntityType::Journal => JournalPayload,
        EntityType::Account => AccountPayload,
        EntityType::Transaction => TransactionPayload,
        EntityType::Passkey => PasskeyPayload,
        EntityType::User => UserPayload,
        // EntityType::Grant => GrantPayload,
        // EntityType::Role => RolePayload,
    )
    // NOTE: Grant and Role entity types do not have an associated payload, they will panic
}

pub trait Store {
    /// Records an event to the store and updates the State
    ///
    /// # Errors
    /// Returns an `EntityType` error if `entity_id.entity_type()` does
    /// not match the `EntityType` of the existing entity in the store
    ///
    /// Returns a `Sequence` error and does not record the event if the latest sequence number
    /// recorded by the store (prior to this event) does not match `expected_sequence`
    async fn record<I: Entity, T: TimeProvider>(
        &self,
        authority: Authority,
        time_provider: &T,
        entity_id: I::Id,
        payload: I::Payload,
        when: When,
    ) -> StoreResult<EventId>;

    /// Returns a vector of all events that have occurred concerning an entity starting at `starting_sequence`
    async fn replay_events<I: Entity>(
        &self,
        entity_id: I::Id,
        after: After,
    ) -> StoreResult<Vec<Event<I>>>;

    /// Returns a State of the given entity and the sequence id associated with the last event applied to it
    async fn get_state<I: Entity>(&self, entity_id: I::Id) -> StoreResult<I::State>;

    /// Rebuilds the State of an entity from the stored events
    async fn rebuild_state<I: Entity>(&self, entity_id: I::Id) -> StoreResult<()>;

    async fn session_store(&self) -> &impl ExpiredDeletion;
}
