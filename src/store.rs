use crate::authority::Authority;
use chrono::DateTime;
use chrono::Utc;
use futures_core::Stream;
use serde::Deserialize;
use serde::Serialize;
use std::error::Error;

type EventId = u64;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Event<P: Clone + Sized, I: Copy + Clone + Sized> {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub authority: Authority,
    pub payload: P,
    pub id: I,
}

#[expect(dead_code)]
pub enum Select<T> {
    All,
    One(T),
}

/// A condition for recording an event.
#[expect(dead_code)]
pub enum When<T> {
    Always,
    Current(T),
}

#[expect(dead_code)]
pub enum After<T> {
    Start,
    Specific(T),
}

/// A page of events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page<P: Clone + Sized, I: Copy + Clone + Sized> {
    pub items: Vec<Event<P, I>>,
    /// Whether there are currently more pages for this query.
    pub more: bool,
    /// The value to use as the `after` param to get the next page for this query.
    pub next: EventId,
}

#[expect(dead_code)]
pub trait Store: Send + Sync {
    type Id: Send + Sync + Copy + Clone;
    type Payload: Send + Sync + Clone;
    type Error: Error;
    type Subscription: Stream<Item = Result<Event<Self::Payload, Self::Id>, Self::Error>>
        + Send
        + 'static;

    /// Record an event to storage.
    ///
    /// # Parameters
    /// - `id`: The id of the resource for this event
    /// - `by`: Who caused this event
    /// - `at`: The time that the event occurred
    /// - `when`: The condition needed to record this event.
    ///   [`When::Current`] avoids writing the event
    ///   if there are new events for this resource
    ///   since the latest read from the store,
    ///   as indicated by the event id given to [`When::Current`].
    /// - `payload`: The specific data needed for this event.
    ///
    /// # Returns
    /// The complete event that was recorded to the store.
    async fn record(
        &self,
        by: Authority,
        at: DateTime<Utc>,
        id: Self::Id,
        payload: Self::Payload,
        when: When<Self::Id>,
    ) -> Result<Event<Self::Payload, Self::Id>, Self::Error>;

    /// Stream events from the event store.
    async fn subscribe(
        &self,
        select: Select<Self::Id>,
        after: After<EventId>,
    ) -> Result<Self::Subscription, Self::Error>;

    /// Get a [`Page`] of events from the store.
    async fn review(
        &self,
        select: Select<Self::Id>,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<Self::Payload, Self::Id>, Self::Error>;
}
