#![allow(dead_code)]

use super::After;
use super::EventFamily;
use super::EventId;
use super::Outcome;
use super::Page;
use super::RecordFor;
use super::Store;
use std::convert::Infallible;
use std::marker::PhantomData;

pub const EVENT_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS event (
    id INTEGER NOT NULL PRIMARY KEY,
    stream_type INTEGER NOT NULL,
    stream_id BLOB NOT NULL,
    timestamp INTEGER NOT NULL,
    authority BLOB NOT NULL,
    payload BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS event_stream_idx
ON event (stream_type, stream_id, id);
"#;

pub trait SqliteStreamId {
    fn stream_type(&self) -> i64;
    fn stream_id(&self) -> &[u8];
}

pub struct EncodedEvent {
    pub event_type: i64,
    pub payload: Vec<u8>,
}

pub trait SqlEventCodec<E: EventFamily> {
    type Error;

    fn encode(event: &E) -> Result<EncodedEvent, Self::Error>;
}

pub struct SqliteStore<E> {
    _event_family: PhantomData<E>,
}

impl<E> SqliteStore<E> {
    pub fn new() -> Self {
        Self {
            _event_family: PhantomData,
        }
    }
}

impl<E> Default for SqliteStore<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E> Store<E> for SqliteStore<E>
where
    E: EventFamily,
    E::Id: SqliteStreamId,
    E::Record: RecordFor<E>,
{
    type Error = Infallible;

    async fn record(
        &self,
        by: E::Authority,
        at: chrono::DateTime<chrono::Utc>,
        record: E::Record,
    ) -> Result<Outcome<E>, Self::Error> {
        self.commit(by, at, vec![record]).await
    }

    async fn commit(
        &self,
        _by: E::Authority,
        _at: chrono::DateTime<chrono::Utc>,
        _records: Vec<E::Record>,
    ) -> Result<Outcome<E>, Self::Error> {
        todo!("implement SQLite-backed atomic commit")
    }

    async fn review(
        &self,
        _id: E::Id,
        _after: After<EventId>,
        _limit: usize,
    ) -> Result<Page<E>, Self::Error> {
        todo!("implement SQLite-backed stream review")
    }

    async fn observe(&self, _after: After<EventId>, _limit: usize) -> Result<Page<E>, Self::Error> {
        todo!("implement SQLite-backed global observe")
    }
}
