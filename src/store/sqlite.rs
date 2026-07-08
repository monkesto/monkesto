#![allow(dead_code)]

use super::After;
use super::EventFamily;
use super::EventFor;
use super::EventId;
use super::Outcome;
use super::Page;
use super::Store;
use chrono::DateTime;
use chrono::Utc;
use sqlx::Row;
use sqlx::SqlitePool;
use std::convert::Infallible;
use std::marker::PhantomData;

pub trait SqliteStreamId {
    fn stream_type(&self) -> i64;
    fn stream_id(&self) -> Vec<u8>;
}

pub struct SqliteEvent {
    pub event_id: EventId,
    pub stream_type: i64,
    pub stream_id: Vec<u8>,
    pub timestamp: DateTime<Utc>,
    pub authority: Vec<u8>,
    pub payload: Vec<u8>,
}

pub trait SqliteEventCodec<E: EventFamily> {
    type Error: std::error::Error + Send + Sync + 'static;

    fn encode_authority(authority: &E::Authority) -> Result<Vec<u8>, Self::Error>;

    fn encode_payload<S>(payload: &S::Payload) -> Result<Vec<u8>, Self::Error>
    where
        S: super::Stream,
        E: EventFor<S>;

    fn decode_event(event: SqliteEvent) -> Result<E, Self::Error>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopSqliteProjection;

pub struct SqliteStatement {
    pub sql: &'static str,
    pub binds: Vec<SqliteValue>,
}

pub enum SqliteValue {
    I64(i64),
    Bytes(Vec<u8>),
    Text(String),
    Bool(bool),
}

pub trait SqliteProjection<E: EventFamily>: Send + Sync {
    type Error: std::error::Error + Send + Sync + 'static;

    fn schema(&self) -> &'static str;

    fn plan(&self, event: &E) -> Result<Vec<SqliteStatement>, Self::Error>;
}

impl<E> SqliteProjection<E> for NoopSqliteProjection
where
    E: EventFamily,
{
    type Error = Infallible;

    fn schema(&self) -> &'static str {
        ""
    }

    fn plan(&self, _event: &E) -> Result<Vec<SqliteStatement>, Self::Error> {
        Ok(Vec::new())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SqliteStoreError<C, P = Infallible> {
    #[error("sqlite error")]
    Sqlite(#[from] sqlx::Error),
    #[error("codec error")]
    Codec(#[source] C),
    #[error("projection error")]
    Projection(#[source] P),
    #[error("invalid event id {0}")]
    InvalidEventId(i64),
    #[error("invalid timestamp {0}")]
    InvalidTimestamp(i64),
}

pub struct SqliteStore<E, P = NoopSqliteProjection> {
    pool: SqlitePool,
    projection: P,
    _event_family: PhantomData<E>,
}

impl<E, P> Clone for SqliteStore<E, P>
where
    P: Clone,
{
    fn clone(&self) -> Self {
        Self {
            pool: self.pool.clone(),
            projection: self.projection.clone(),
            _event_family: PhantomData,
        }
    }
}

impl<E, P> SqliteStore<E, P>
where
    P: SqliteProjection<E>,
    E: EventFamily,
{
    pub async fn new(pool: SqlitePool, projection: P) -> Result<Self, sqlx::Error> {
        const CREATE_EVENT_TABLE: &str = r#"
        CREATE TABLE IF NOT EXISTS event (
            id INTEGER NOT NULL PRIMARY KEY,
            stream_type INTEGER NOT NULL,
            stream_id BLOB NOT NULL,
            timestamp INTEGER NOT NULL,
            authority BLOB NOT NULL,
            payload BLOB NOT NULL
        )
        "#;
        const CREATE_EVENT_STREAM_INDEX: &str = r#"
        CREATE INDEX IF NOT EXISTS event_stream_idx
        ON event (stream_type, stream_id, id)
        "#;

        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA synchronous = NORMAL")
            .execute(&pool)
            .await?;
        sqlx::query(CREATE_EVENT_TABLE).execute(&pool).await?;
        sqlx::query(CREATE_EVENT_STREAM_INDEX)
            .execute(&pool)
            .await?;

        sqlx::query(projection.schema()).execute(&pool).await?;

        Ok(Self {
            pool,
            projection,
            _event_family: PhantomData,
        })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub fn projection(&self) -> &P {
        &self.projection
    }
}

impl<E, P> Store<E> for SqliteStore<E, P>
where
    E: EventFamily + SqliteEventCodec<E>,
    E::Id: SqliteStreamId,
    P: SqliteProjection<E>,
{
    type Error = SqliteStoreError<E::Error, P::Error>;

    async fn record<S>(
        &self,
        by: E::Authority,
        at: chrono::DateTime<chrono::Utc>,
        id: S::Id,
        payload: S::Payload,
        when: super::When<EventId>,
    ) -> Result<Outcome<E>, Self::Error>
    where
        S: super::Stream,
        E: EventFor<S>,
    {
        let family_id = E::id_for(id);
        let stream_type = family_id.stream_type();
        let stream_id = family_id.stream_id();
        let expected_latest = match when {
            super::When::Empty => 0,
            super::When::Within(event_id) => event_id_i64(event_id)?,
        };
        let authority = E::encode_authority(&by).map_err(SqliteStoreError::Codec)?;
        let encoded_payload = E::encode_payload::<S>(&payload).map_err(SqliteStoreError::Codec)?;

        let mut tx = self.pool.begin().await?;
        let row = sqlx::query(
            r#"
            INSERT INTO event (
                stream_type,
                stream_id,
                timestamp,
                authority,
                payload
            )
            SELECT ?, ?, ?, ?, ?
            WHERE (
                SELECT COALESCE(MAX(id), 0)
                FROM event
                WHERE stream_type = ? AND stream_id = ?
            ) <= ?
            RETURNING id
            "#,
        )
        .bind(stream_type)
        .bind(stream_id.clone())
        .bind(at.timestamp_millis())
        .bind(authority.clone())
        .bind(encoded_payload.clone())
        .bind(stream_type)
        .bind(stream_id.clone())
        .bind(expected_latest)
        .fetch_optional(&mut *tx)
        .await?;

        let Some(row) = row else {
            tx.commit().await?;
            return Ok(Outcome::Skipped);
        };

        let event_id = event_id_from_i64(row.get("id"))?;
        let event = E::decode_event(SqliteEvent {
            event_id,
            stream_type,
            stream_id,
            timestamp: at,
            authority,
            payload: encoded_payload,
        })
        .map_err(SqliteStoreError::Codec)?;

        for statement in self
            .projection
            .plan(&event)
            .map_err(SqliteStoreError::Projection)?
        {
            execute_statement(&mut tx, statement).await?;
        }
        tx.commit().await?;

        Ok(Outcome::Recorded(event))
    }

    async fn review(
        &self,
        id: E::Id,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<E>, Self::Error> {
        let after = after_i64(after)?;
        self.fetch_page(
            r#"
            SELECT id, stream_type, stream_id, timestamp, authority, payload
            FROM event
            WHERE stream_type = ? AND stream_id = ? AND id > ?
            ORDER BY id ASC
            LIMIT ?
            "#,
            Some((id.stream_type(), id.stream_id())),
            after,
            limit,
        )
        .await
    }

    async fn observe(&self, after: After<EventId>, limit: usize) -> Result<Page<E>, Self::Error> {
        let after = after_i64(after)?;
        self.fetch_page(
            r#"
            SELECT id, stream_type, stream_id, timestamp, authority, payload
            FROM event
            WHERE id > ?
            ORDER BY id ASC
            LIMIT ?
            "#,
            None,
            after,
            limit,
        )
        .await
    }
}

impl<E, P> SqliteStore<E, P>
where
    E: EventFamily + SqliteEventCodec<E>,
    E::Id: SqliteStreamId,
    P: SqliteProjection<E>,
{
    async fn fetch_page(
        &self,
        query: &'static str,
        stream: Option<(i64, Vec<u8>)>,
        after: i64,
        limit: usize,
    ) -> Result<Page<E>, SqliteStoreError<E::Error, P::Error>> {
        let fetch_limit = i64::try_from(limit.saturating_add(1)).unwrap_or(i64::MAX);
        let rows = if let Some((stream_type, stream_id)) = stream {
            sqlx::query(query)
                .bind(stream_type)
                .bind(stream_id)
                .bind(after)
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await?
        } else {
            sqlx::query(query)
                .bind(after)
                .bind(fetch_limit)
                .fetch_all(&self.pool)
                .await?
        };

        let more = rows.len() > limit;
        let items = rows
            .into_iter()
            .take(limit)
            .map(|row| {
                let timestamp = row.get("timestamp");
                let event = SqliteEvent {
                    event_id: event_id_from_i64(row.get("id"))?,
                    stream_type: row.get("stream_type"),
                    stream_id: row.get("stream_id"),
                    timestamp: DateTime::from_timestamp_millis(timestamp)
                        .ok_or(SqliteStoreError::InvalidTimestamp(timestamp))?,
                    authority: row.get("authority"),
                    payload: row.get("payload"),
                };
                E::decode_event(event).map_err(SqliteStoreError::Codec)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let latest = self.latest_event_id().await?;
        let next = items.last().map(EventFamily::event_id).unwrap_or(latest);

        Ok(Page { items, more, next })
    }

    async fn latest_event_id(&self) -> Result<EventId, SqliteStoreError<E::Error, P::Error>> {
        let row = sqlx::query("SELECT COALESCE(MAX(id), 0) AS id FROM event")
            .fetch_one(&self.pool)
            .await?;
        event_id_from_i64(row.get("id"))
    }
}

async fn execute_statement(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    statement: SqliteStatement,
) -> Result<(), sqlx::Error> {
    let mut query = sqlx::query(statement.sql);
    for value in statement.binds {
        query = match value {
            SqliteValue::I64(value) => query.bind(value),
            SqliteValue::Bytes(value) => query.bind(value),
            SqliteValue::Text(value) => query.bind(value),
            SqliteValue::Bool(value) => query.bind(value),
        };
    }
    query.execute(&mut **tx).await?;
    Ok(())
}

fn after_i64<C, P>(after: After<EventId>) -> Result<i64, SqliteStoreError<C, P>> {
    match after {
        After::Start => Ok(0),
        After::Specific(event_id) => event_id_i64(event_id),
    }
}

fn event_id_i64<C, P>(event_id: EventId) -> Result<i64, SqliteStoreError<C, P>> {
    i64::try_from(*event_id).map_err(|_| SqliteStoreError::InvalidEventId(i64::MAX))
}

fn event_id_from_i64<C, P>(event_id: i64) -> Result<EventId, SqliteStoreError<C, P>> {
    let event_id =
        u64::try_from(event_id).map_err(|_| SqliteStoreError::InvalidEventId(event_id))?;
    Ok(EventId::from(event_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Event;
    use crate::store::EventFor;
    use crate::store::EventId;
    use crate::store::Stream;
    use sqlx::sqlite::SqliteConnectOptions;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::convert::Infallible;
    use std::str::FromStr;
    use tempfile::TempDir;

    struct TestSqliteStore<E> {
        _dir: TempDir,
        store: SqliteStore<E>,
    }

    impl<E> TestSqliteStore<E>
    where
        E: EventFamily,
    {
        async fn new() -> Self {
            let dir = tempfile::tempdir().expect("tempdir should be created");
            let path = dir.path().join("store.sqlite");
            let url = format!("sqlite://{}", path.display());
            let connection_options = SqliteConnectOptions::from_str(&url)
                .expect("sqlite url should parse")
                .create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                .max_connections(5)
                .connect_with(connection_options)
                .await
                .expect("sqlite pool should connect");
            let store = SqliteStore::new(pool, NoopSqliteProjection)
                .await
                .expect("sqlite store should initialize");
            Self { _dir: dir, store }
        }
    }

    impl<E> Store<E> for TestSqliteStore<E>
    where
        E: EventFamily + SqliteEventCodec<E>,
        E::Id: SqliteStreamId,
    {
        type Error = SqliteStoreError<E::Error>;

        async fn record<S>(
            &self,
            by: E::Authority,
            at: DateTime<Utc>,
            id: S::Id,
            payload: S::Payload,
            when: super::super::When<EventId>,
        ) -> Result<Outcome<E>, Self::Error>
        where
            S: Stream,
            E: EventFor<S>,
        {
            self.store.record::<S>(by, at, id, payload, when).await
        }

        async fn review(
            &self,
            id: E::Id,
            after: After<EventId>,
            limit: usize,
        ) -> Result<Page<E>, Self::Error> {
            self.store.review(id, after, limit).await
        }

        async fn observe(
            &self,
            after: After<EventId>,
            limit: usize,
        ) -> Result<Page<E>, Self::Error> {
            self.store.observe(after, limit).await
        }
    }

    impl SqliteStreamId for store_contract::TestId {
        fn stream_type(&self) -> i64 {
            match self {
                store_contract::TestId::Alpha(_) => 1,
                store_contract::TestId::Beta(_) => 2,
            }
        }

        fn stream_id(&self) -> Vec<u8> {
            match self {
                store_contract::TestId::Alpha(id) | store_contract::TestId::Beta(id) => {
                    id.to_be_bytes().to_vec()
                }
            }
        }
    }

    impl SqliteEventCodec<store_contract::TestEvent> for store_contract::TestEvent {
        type Error = Infallible;

        fn encode_authority(authority: &i32) -> Result<Vec<u8>, Self::Error> {
            Ok(authority.to_be_bytes().to_vec())
        }

        fn encode_payload<S>(payload: &S::Payload) -> Result<Vec<u8>, Self::Error>
        where
            S: Stream,
            store_contract::TestEvent: EventFor<S>,
        {
            let payload = (payload as &dyn std::any::Any)
                .downcast_ref::<&'static str>()
                .expect("test payload should be a static string");
            Ok(payload.as_bytes().to_vec())
        }

        fn decode_event(event: SqliteEvent) -> Result<store_contract::TestEvent, Self::Error> {
            let authority = i32::from_be_bytes(
                event
                    .authority
                    .as_slice()
                    .try_into()
                    .expect("authority should have four bytes"),
            );
            let id = u32::from_be_bytes(
                event
                    .stream_id
                    .as_slice()
                    .try_into()
                    .expect("stream id should have four bytes"),
            );
            let payload = match event.payload.as_slice() {
                b"first" => "first",
                b"second" => "second",
                b"third" => "third",
                b"a1" => "a1",
                b"b1" => "b1",
                _ => panic!("unexpected payload"),
            };

            Ok(match event.stream_type {
                1 => store_contract::TestEvent::Alpha(Event {
                    event_id: event.event_id,
                    timestamp: event.timestamp,
                    authority,
                    id,
                    payload,
                }),
                2 => store_contract::TestEvent::Beta(Event {
                    event_id: event.event_id,
                    timestamp: event.timestamp,
                    authority,
                    id,
                    payload,
                }),
                _ => panic!("unexpected stream type"),
            })
        }
    }

    #[derive(Clone)]
    struct RecordingProjection;

    impl SqliteProjection<store_contract::TestEvent> for RecordingProjection {
        type Error = Infallible;

        fn schema(&self) -> &'static str {
            r#"
            CREATE TABLE IF NOT EXISTS test_projection (
                event_id INTEGER NOT NULL PRIMARY KEY,
                payload BLOB NOT NULL
            )
            "#
        }

        fn plan(
            &self,
            event: &store_contract::TestEvent,
        ) -> Result<Vec<SqliteStatement>, Self::Error> {
            let payload = match event {
                store_contract::TestEvent::Alpha(event) => event.payload.as_bytes(),
                store_contract::TestEvent::Beta(event) => event.payload.as_bytes(),
            };

            Ok(vec![SqliteStatement {
                sql: r#"
                INSERT INTO test_projection (event_id, payload)
                VALUES (?, ?)
                "#,
                binds: vec![
                    SqliteValue::I64(
                        i64::try_from(*event.event_id()).expect("event id should fit"),
                    ),
                    SqliteValue::Bytes(payload.to_vec()),
                ],
            }])
        }
    }

    #[derive(Clone)]
    struct FailingProjection;

    impl SqliteProjection<store_contract::TestEvent> for FailingProjection {
        type Error = Infallible;

        fn schema(&self) -> &'static str {
            ""
        }

        fn plan(
            &self,
            _event: &store_contract::TestEvent,
        ) -> Result<Vec<SqliteStatement>, Self::Error> {
            Ok(vec![SqliteStatement {
                sql: r#"
                INSERT INTO definitely_missing_projection_table (event_id)
                VALUES (?)
                "#,
                binds: vec![SqliteValue::I64(1)],
            }])
        }
    }

    #[tokio::test]
    async fn record_commits_event_and_projection_in_one_transaction() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("store.sqlite");
        let url = format!("sqlite://{}", path.display());
        let connection_options = SqliteConnectOptions::from_str(&url)
            .expect("sqlite url should parse")
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(connection_options)
            .await
            .expect("sqlite pool should connect");
        let store = SqliteStore::<store_contract::TestEvent, RecordingProjection>::new(
            pool,
            RecordingProjection,
        )
        .await
        .expect("sqlite store should initialize");

        store
            .record::<store_contract::AlphaStream>(
                0,
                Utc::now(),
                1u32,
                "first",
                super::super::When::Empty,
            )
            .await
            .expect("record should succeed");

        let event_page = store
            .observe(After::Start, 10)
            .await
            .expect("events should load");
        assert_eq!(event_page.items.len(), 1);

        let projection_count: i64 = sqlx::query(
            r#"
            SELECT COUNT(*) AS count
            FROM test_projection
            "#,
        )
        .fetch_one(store.pool())
        .await
        .expect("projection count should load")
        .get("count");
        assert_eq!(projection_count, 1);
    }

    #[tokio::test]
    async fn record_rolls_back_event_when_projection_statement_fails() {
        let dir = tempfile::tempdir().expect("tempdir should be created");
        let path = dir.path().join("store.sqlite");
        let url = format!("sqlite://{}", path.display());
        let connection_options = SqliteConnectOptions::from_str(&url)
            .expect("sqlite url should parse")
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(connection_options)
            .await
            .expect("sqlite pool should connect");
        let store = SqliteStore::<store_contract::TestEvent, FailingProjection>::new(
            pool,
            FailingProjection,
        )
        .await
        .expect("sqlite store should initialize");

        let result = store
            .record::<store_contract::AlphaStream>(
                0,
                Utc::now(),
                1u32,
                "first",
                super::super::When::Empty,
            )
            .await;
        assert!(matches!(result, Err(SqliteStoreError::Sqlite(_))));

        let event_page = store
            .observe(After::Start, 10)
            .await
            .expect("events should load");
        assert!(event_page.items.is_empty());
    }

    store_contract_tests!(TestSqliteStore::<TestEvent>::new().await);
}
