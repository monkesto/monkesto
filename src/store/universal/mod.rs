use crate::authority::Authority;
use crate::ident::{Ident, ProjectionFromPayloadError};
use crate::store::universal::registry::{AnyPayload, EntityType};
use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::ops::{Add, Deref};
use thiserror::Error;
use tower_sessions::SessionStore;

mod example_entity;
pub mod registry;
mod simple_sqlite;

#[derive(Debug, Error, Clone, Deserialize)]
pub enum StoreError {
    #[error("failed to deserialize an event payload")]
    Deserialize(#[from] postcard::Error),

    #[error("sequence error: expected {expected:?}, found {found:?}")]
    Sequence {
        expected: SequenceId,
        found: SequenceId,
    },

    #[error("incorrect entity type: expected {expected:?}, found {found:?}")]
    EntityType {
        expected: Option<EntityType>,
        found: Option<EntityType>,
    },

    #[error("sqlx returned an error: {0}")]
    Sqlx(String),

    #[error(transparent)]
    ProjectionFromPayload(#[from] ProjectionFromPayloadError),

    #[error("attempted to apply an update to the transaction {0}, but it doesn't exist")]
    TransactionModifiedBeforeCreation(Ident),

    #[error("attempted to apply an update to the transaction {0}, but it was deleted")]
    TransactionModifiedAfterDeletion(Ident),

    #[error("attempted to apply an update to the account {0}, but it doesn't exist")]
    AccountModifiedBeforeCreation(Ident),
}
impl From<sqlx::Error> for StoreError {
    fn from(error: sqlx::Error) -> Self {
        StoreError::Sqlx(error.to_string())
    }
}

pub type StoreResult<T> = Result<T, StoreError>;

#[derive(Debug)]
pub struct PayloadWithId<T: Entity> {
    pub payload: T::Payload,
    pub id: T::Id,
}

pub trait EntityId: Deref<Target = Ident> + Copy {
    fn as_bytes(&self) -> &[u8];
}

pub trait Payload: Send + Sync + Clone + Serialize + DeserializeOwned {
    fn as_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("Failed to serialize payload")
    }
    fn from_bytes(bytes: &[u8]) -> StoreResult<Self> {
        postcard::from_bytes(bytes)?
    }

    fn creates_entity(&self) -> bool;
}

pub trait ApplyPayload<T: Entity> {
    fn apply(&mut self, payload: &T::Payload) -> &mut T::Projection;
}

pub trait Entity: Sized {
    type Id: EntityId;
    type Payload: Payload + Into<AnyPayload>;
    type Projection: Projection<Self>;

    fn entity_type() -> EntityType;
}

pub trait Projection<T: Entity>:
    Send
    + Sync
    + Clone
    + TryFrom<PayloadWithId<T>, Error = ProjectionFromPayloadError>
    + Serialize
    + DeserializeOwned
    + ApplyPayload<T>
{
    fn as_bytes(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("Failed to serialize payload")
    }

    fn from_bytes(bytes: &[u8]) -> StoreResult<Self> {
        postcard::from_bytes(bytes)?
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Eq)]
pub struct EventId(u64);

impl Deref for EventId {
    type Target = u64;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Eq)]
pub struct SequenceId(u64);
impl Deref for SequenceId {
    type Target = u64;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<i64> for SequenceId {
    fn from(id: i64) -> Self {
        SequenceId(id as u64)
    }
}

impl Add<u64> for SequenceId {
    type Output = Self;

    fn add(self, rhs: u64) -> Self::Output {
        Self(self.0.checked_add(rhs).expect("SequenceId overflow"))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event<I: Entity> {
    pub event_id: EventId,
    pub sequence_id: SequenceId,
    pub timestamp: DateTime<Utc>,
    pub authority: Authority,
    pub entity_id: I,
    pub payload: I::Payload,
}

pub trait Store {
    /// Records an event to the store and updates the projection
    ///
    /// # Errors
    /// Returns an `EntityType` error if `entity_id.entity_type()` does
    /// not match the `EntityType` of the existing entity in the store
    ///
    /// Returns a `Sequence` error and does not record the event if the latest sequence number
    /// recorded by the store (prior to this event) does not match `expected_sequence`
    async fn record<I: Entity>(
        &self,
        authority: Authority,
        at: DateTime<Utc>,
        entity_id: I::Id,
        payload: I::Payload,
        expected_sequence: SequenceId,
    ) -> StoreResult<EventId>;

    /// Returns a vector of all events that have occurred concerning an entity starting at `starting_sequence`
    async fn replay_events<I: Entity>(
        &self,
        entity_id: I::Id,
        starting_sequence: SequenceId,
    ) -> Vec<Event<I>>;

    /// Returns a projection of the given entity and the sequence id associated with the last event applied to it
    async fn get_projection<I: Entity>(
        &self,
        entity_id: I::Id,
    ) -> StoreResult<(I::Projection, SequenceId)>;

    /// Rebuilds the projection of an entity with the given events
    async fn rebuild_projection<I: Entity>(
        &self,
        entity_id: I::Id,
        events: Vec<Event<I>>,
    ) -> StoreResult<()>;

    async fn session_store(&self) -> &impl SessionStore;
}
