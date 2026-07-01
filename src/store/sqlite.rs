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

/// Initial event table shape for a SQLite-backed `Store<E>`.
///
/// The table is scoped to one event family for now. `stream_type` and
/// `stream_id` form the stream key used by `When` checks and `review`.
/// `event_type` makes the decoder key explicit; `payload` is decoded by the
/// event family's SQLite codec.
pub const EVENT_SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS events (
    event_id INTEGER NOT NULL PRIMARY KEY,
    stream_type INTEGER NOT NULL,
    stream_id BLOB NOT NULL,
    event_type INTEGER NOT NULL,
    timestamp BIGINT NOT NULL,
    authority BLOB NOT NULL,
    payload BLOB NOT NULL
);

CREATE INDEX IF NOT EXISTS events_stream_idx
ON events (stream_type, stream_id, event_id);
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
