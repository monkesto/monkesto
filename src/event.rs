use crate::authority::Authority;
use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::error::Error;

pub trait EventStore: Send + Sync {
    type Id: Send + Sync + Clone + Copy;
    type EventId: Send + Sync + Clone + Copy;
    type Payload: Send + Sync + Clone;
    type Error: Error;

    /// Record an event to storage.
    ///
    /// # Parameters
    /// - `id`: The aggregate/entity identifier (what this event is about)
    /// - `by`: The authority (who or what caused this event)
    /// - `event`: The domain-specific event payload
    ///
    /// # Returns
    /// The store-generated event identifier on success
    async fn record(
        &self,
        id: Self::Id,
        by: Authority,
        payload: Self::Payload,
    ) -> Result<Self::EventId, Self::Error>;

    /// Get all events after the specified event number
    ///
    /// # Parameters
    /// - `id`: The id of the aggregate
    /// - `after`: The start event version number (exclusive)
    /// - `limit`: The maximum number of events to return
    // TODO: Use an event id of some sort instead of a usize
    #[allow(dead_code)]
    async fn get_events(
        &self,
        id: Self::Id,
        after: Self::EventId,
        limit: Self::EventId,
    ) -> Result<Vec<Event<Self::Payload, Self::Id>>, Self::Error>;
}

#[expect(dead_code)]
pub trait ViewModel {
    type Id: Send + Sync + Clone;

    type Receiver: Send + Sync;
    type EventId: Send + Sync + Clone;
    type Event: Send + Sync;
    type Error: Error;

    /// Subscribes to a receiver provided by the event store
    /// This function should never return; it should loop forever waiting to receive events
    async fn subscribe_events(&self, receiver: Self::Receiver) -> Result<(), Self::Error>;

    // TODO: setup a function to wait for a specific event to be received and a manual insert function
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Event<T: Clone + Sized, U: Copy + Clone + Sized> {
    pub payload: T,
    pub id: U,
    pub event_id: u64,
    pub timestamp: DateTime<Utc>,
    pub authority: Authority,
}

impl<T: Clone, U: Copy + Clone> Event<T, U> {
    pub fn new(payload: T, id: U, event_id: u64, authority: Authority) -> Self {
        Self {
            payload,
            id,
            event_id,
            timestamp: Utc::now(),
            authority,
        }
    }
}
