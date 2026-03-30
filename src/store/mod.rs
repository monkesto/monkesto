use crate::authority::Authority;
use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::error::Error;
use std::ops::Deref;

pub mod memory;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventId(u64);

impl EventId {
    pub fn next(&self) -> Self {
        EventId(self.0 + 1)
    }
}

impl Deref for EventId {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u64> for EventId {
    fn from(value: u64) -> Self {
        EventId(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Event<I: Copy + Clone + Sized, P: Clone + Sized> {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub authority: Authority,
    pub id: I,
    pub payload: P,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<I: Copy + Clone + Sized, P: Clone + Sized> {
    Recorded(Event<I, P>),
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Select<T: Copy> {
    #[expect(dead_code)]
    All,
    #[expect(dead_code)]
    One(T),
}

/// A condition for recording an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When<T: Copy> {
    #[expect(dead_code)]
    Always,
    #[expect(dead_code)]
    Current(T),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After<T: Copy> {
    #[expect(dead_code)]
    Start,
    #[expect(dead_code)]
    Specific(T),
}

/// A page of events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page<I: Copy + Clone + Sized, P: Clone + Sized> {
    pub items: Vec<Event<I, P>>,
    /// Whether there are currently more pages for this query.
    pub more: bool,
    /// The value to use as the `after` param to get the next page for this query.
    pub next: EventId,
}

#[expect(dead_code)]
pub trait Store: Send + Sync {
    type Id: Send + Sync + Copy + Clone;
    type Payload: Send + Sync + Clone;
    type Error: Error + Send + Sync + 'static;

    async fn record(
        &self,
        by: Authority,
        at: DateTime<Utc>,
        id: Self::Id,
        payload: Self::Payload,
        when: When<EventId>,
    ) -> Result<Outcome<Self::Id, Self::Payload>, Self::Error>;

    async fn review(
        &self,
        select: Select<Self::Id>,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<Self::Id, Self::Payload>, Self::Error>;
}
