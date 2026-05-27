use crate::authority::Authority;
use crate::ident::Ident;
use crate::postcard::Postcard;
use crate::store::universal::error::StoreResult;
use crate::store::universal::registry::{AnyPayload, EntityType};
use crate::store::universal::time_provider::TimeProvider;
use diesel::sql_types::Binary;
use diesel::{Insertable, Queryable, QueryableByName, Selectable, SqliteConnection};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::ops::Deref;
use tower_sessions::ExpiredDeletion;

mod diesel_sqlite;
pub mod error;
pub mod event_id;
mod example_entity;
pub mod registry;
pub mod time_provider;
pub mod timestamp;

pub use event_id::EventId;
pub use timestamp::TimeStamp;

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
