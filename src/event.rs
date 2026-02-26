use crate::authority::Authority;
use std::error::Error;

pub trait EventStore: Send + Sync {
    type Id: Send + Sync + Clone;
    type EventId: Send + Sync + Clone;
    type Event: Send + Sync;
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
        event: Self::Event,
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
        after: usize,
        limit: usize,
    ) -> Result<Vec<Self::Event>, Self::Error>;
}
