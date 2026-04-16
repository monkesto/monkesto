use crate::account::{AccountPayload, AccountProjection};
use crate::auth::passkey::{PasskeyId, PasskeyPayload, PasskeyProjection};
use crate::auth::user::{UserPayload, UserProjection};
use crate::authority::{Authority, UserId};
use crate::grant::GrantPayload;
use crate::ident::{AccountId, EntityId, JournalId, TransactionId};
use crate::journal::{JounalPayload, JournalProjection};
use crate::role::RolePayload;
use crate::transaction::{TransactionPayload, TransactionProjection};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::any::{Any, TypeId};
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

pub trait Payload<'a>: Send + Sync + Clone + Serialize + Deserialize<'a> {
    fn serialize(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("Failed to serialize payload")
    }
    fn from_bytes(bytes: &'a [u8]) -> StoreResult<Self> {
        postcard::from_bytes(bytes)?
    }
}

impl Payload<'_> for TransactionPayload {}
impl Payload<'_> for AccountPayload {}
impl Payload<'_> for PasskeyPayload {}
impl Payload<'_> for JounalPayload {}
impl Payload<'_> for GrantPayload {}
impl Payload<'_> for RolePayload {}
impl Payload<'_> for UserPayload {}

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
pub trait Projection: Send + Sync + Clone {}

impl Projection for AccountProjection {}
impl Projection for JournalProjection {}
impl Projection for TransactionProjection {}
impl Projection for PasskeyProjection {}
impl Projection for UserProjection {}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub enum EntityType {
    Journal,
    Account,
    Transaction,
    Passkey,
    User,
}

impl EntityType {
    pub fn from_entity_id(id: impl EntityId<'static> + 'static) -> Option<Self> {
        use EntityType::*;
        match id.type_id() {
            t if t == TypeId::of::<JournalId>() => Some(Journal),
            t if t == TypeId::of::<AccountId>() => Some(Account),
            t if t == TypeId::of::<TransactionId>() => Some(Transaction),
            t if t == TypeId::of::<PasskeyId>() => Some(Passkey),
            t if t == TypeId::of::<UserId>() => Some(User),
            _ => None,
        }
    }
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
    /// Returns an `EntityType` error if `EntityType::from_entity_id(entity_id)` does
    /// not match the `EntityType` of the existing entity in the store
    ///
    /// Returns a `Sequence` error if the store-assigned sequence number
    /// of the event does not match the `expected_sequence`
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
        entity_id: I,
        starting_sequence: SequenceId,
    ) -> Vec<Event<'a, I>>;
}
