#![expect(dead_code)]

use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::error::Error;
use std::ops::Deref;

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

pub trait Stream {
    type Id: Send + Sync + Copy + Clone;
    type Payload: Send + Sync + Clone;
}

pub trait StreamFamily: Send + Sync + Clone {
    type Id: Send + Sync + Copy + Clone;
    type Record: Send + Sync + Clone;
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Event<A: Clone + Sized, S: Stream>
where
    S::Id: Sized,
    S::Payload: Sized,
{
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub authority: A,
    pub id: S::Id,
    pub payload: S::Payload,
}

pub trait EventFamily: Send + Sync + Clone {
    type Stream: StreamFamily;
    type Authority: Send + Sync + Clone;

    fn event_id(&self) -> EventId;
    fn id(&self) -> <Self::Stream as StreamFamily>::Id;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<E> {
    Recorded(Vec<E>),
    Skipped,
}

pub struct Record<S: Stream> {
    pub id: S::Id,
    pub payload: S::Payload,
    pub when: When<EventId>,
}

impl<S: Stream> Clone for Record<S> {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            payload: self.payload.clone(),
            when: self.when,
        }
    }
}

pub trait Store<E: EventFamily>: Send + Sync {
    type Error: Error + Send + Sync + 'static;

    async fn record(
        &self,
        by: E::Authority,
        at: DateTime<Utc>,
        record: <E::Stream as StreamFamily>::Record,
    ) -> Result<Outcome<E>, Self::Error>;

    async fn commit(
        &self,
        by: E::Authority,
        at: DateTime<Utc>,
        records: Vec<<E::Stream as StreamFamily>::Record>,
    ) -> Result<Outcome<E>, Self::Error>;

    async fn review(
        &self,
        id: <E::Stream as StreamFamily>::Id,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<E>, Self::Error>;

    #[rustfmt::skip]
    async fn observe(
        &self,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<E>, Self::Error>;
}
