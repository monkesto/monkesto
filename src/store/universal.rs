use crate::auth::user::Email;
use crate::authority::Authority;
use crate::ident::EntityId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::ops::Deref;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Deserialize)]
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
        expected: EntityType,
        found: Option<EntityType>,
    },
}

pub type StoreResult<T> = Result<T, StoreError>;

pub trait EmailUpdate {
    /// returns an email if the payload modifies it in any way
    fn email(&self) -> Option<&Email>;
}

#[derive(Debug)]
pub struct PayloadWithId<'a, T: EntityId<'a>> {
    pub payload: T::Payload,
    pub id: T,
}

pub trait Payload<'a>: Send + Sync + Clone + Serialize + Deserialize<'a> + EmailUpdate {
    fn serialize(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("Failed to serialize payload")
    }
    fn from_bytes(bytes: &'a [u8]) -> StoreResult<Self> {
        postcard::from_bytes(bytes)?
    }

    fn creates_entity(&self) -> bool;
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
#[repr(i8)]
#[derive(Debug, Clone, PartialEq, Deserialize, sqlx::Type)]
pub enum EntityType {
    Journal = 0,
    Account = 1,
    Transaction = 2,
    Passkey = 3,
    User = 4,
    Grant = 5,
    Role = 6,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event<'a, I: EntityId<'a>> {
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
    async fn record<'a, I: EntityId<'a>>(
        &self,
        by: Authority,
        at: DateTime<Utc>,
        entity_id: I,
        payload: I::Payload,
        expected_sequence: SequenceId,
    ) -> StoreResult<EventId>;

    /// Returns a vector of all events that have occurred concerning an entity starting at `starting_sequence`
    async fn replay_events<'a, I: EntityId<'a>>(
        &self,
        entity_id: I,
        starting_sequence: SequenceId,
    ) -> Vec<Event<'a, I>>;

    /// Returns a projection of the given entity and the sequence id associated with the last event applied to it
    async fn get_projection<'a, I: EntityId<'a>>(
        &self,
        entity_id: I,
    ) -> StoreResult<(I::Projection, SequenceId)>;

    /// Rebuilds the projection of an entity with the given events
    async fn rebuild_projection<'a, I: EntityId<'a>>(
        &self,
        entity_id: I,
        events: Vec<Event<'a, I>>,
    ) -> StoreResult<()>;
}
