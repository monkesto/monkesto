use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::error::Error;
use std::hash::Hash;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When<T: Copy> {
    Empty,
    Within(T),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After<T: Copy> {
    Start,
    Specific(T),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub more: bool,
    pub next: EventId,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Event<A, I, P> {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub authority: A,
    pub id: I,
    pub payload: P,
}

pub trait RecordFor<E: EventFamily>: Send + Sync + Clone {
    fn id(&self) -> E::Id;
    fn when(&self) -> When<EventId>;
    fn into_event(self, event_id: EventId, authority: E::Authority, timestamp: DateTime<Utc>) -> E;
}

pub trait EventFamily: Send + Sync + Clone + Sized {
    type Id: Send + Sync + Copy + Clone + Eq + Hash;
    type Record: RecordFor<Self>;
    type Authority: Send + Sync + Clone;

    fn event_id(&self) -> EventId;
    fn id(&self) -> Self::Id;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<E> {
    Recorded(Vec<E>),
    Skipped,
}

#[derive(Clone)]
pub struct Record<I, P> {
    pub id: I,
    pub payload: P,
    pub when: When<EventId>,
}

pub trait Store<E: EventFamily>: Send + Sync {
    type Error: Error + Send + Sync + 'static;

    async fn record(
        &self,
        by: E::Authority,
        at: DateTime<Utc>,
        record: E::Record,
    ) -> Result<Outcome<E>, Self::Error>;

    async fn commit(
        &self,
        by: E::Authority,
        at: DateTime<Utc>,
        records: Vec<E::Record>,
    ) -> Result<Outcome<E>, Self::Error>;

    async fn review(
        &self,
        id: E::Id,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<E>, Self::Error>;

    #[rustfmt::skip]
    #[expect(dead_code)]
    async fn observe(
        &self,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<E>, Self::Error>;
}
