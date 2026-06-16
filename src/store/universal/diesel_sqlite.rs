use crate::authority::Authority;
use crate::ident::Ident;
use crate::postcard::Postcard;
use crate::schema::sessions;
use crate::schema::{entities, events};
use crate::store::universal::diesel_sqlite_interface::{
    DieselSqliteAccountInterface, DieselSqliteAuthInterface, DieselSqliteJournalInterface,
    DieselSqliteTransactionInterface,
};
use crate::store::universal::error::StoreError;
use crate::store::universal::registry::{AnyPayload, EntityType, payload_from_bytes};
use crate::store::universal::time_provider::{DefaultTimeProvider, TimeProvider};
use crate::store::universal::{
    After, DieselExecute, DieselFetchState, Entity, Event, EventId, Page, Store, StoreResult,
    TimeStamp, When,
};
use async_trait::async_trait;
use deadpool_diesel::Runtime;
use deadpool_diesel::sqlite::Object;
use deadpool_diesel::{Manager, Pool};
use diesel::connection::SimpleConnection;
use diesel::result::DatabaseErrorKind;
use diesel::sql_types::{BigInt, Binary, Bool};
use diesel::upsert::excluded;
use diesel::{Connection, JoinOnDsl, QueryDsl, QueryableByName, RunQueryDsl};
use diesel::{ExpressionMethods, OptionalExtension};
use diesel::{Insertable, Queryable, Selectable, SqliteConnection};
use diesel_migrations::MigrationHarness;
use diesel_migrations::{EmbeddedMigrations, embed_migrations};
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{mpsc, watch};
use tower_sessions::cookie::time::OffsetDateTime;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::Error::Backend;
use tower_sessions::{ExpiredDeletion, SessionStore, session_store};

#[derive(Clone)]
pub struct DieselSqliteStore {
    pub pool: Pool<Manager<SqliteConnection>>,
    //event_tx: mpsc::Sender<(EventId, Box<AnyPayload>, Ident)>,
    // processed_rx: watch::Receiver<EventId>,
}

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/");

impl DieselSqliteStore {
    pub async fn new(url: &str, max_size: usize) -> DieselSqliteStore {
        let manager = Manager::new(url, Runtime::Tokio1);

        let pool = Pool::builder(manager)
            .max_size(max_size)
            .build()
            .expect("failed to initialize Diesel connection pool");

        // let (event_tx, event_rx) = mpsc::channel::<(EventId, Box<AnyPayload>, Ident)>(64);

        let conn: Object = pool
            .get()
            .await
            .expect("couldn't get a connection from pool");

        let _latest_applied_event = conn
            .interact(|conn| {
                conn.run_pending_migrations(MIGRATIONS)
                    .expect("failed to run migrations");
                conn.batch_execute("PRAGMA journal_mode = WAL;")
                    .expect("failed to enable WAL mode");
                conn.batch_execute("PRAGMA synchronous = NORMAL;")
                    .expect("failed to set synchronous mode");
                conn.batch_execute("PRAGMA foreign_keys = ON;")
                    .expect("failed to enable foreign keys");
                conn.batch_execute("PRAGMA busy_timeout = 250;")
                    .expect("failed to set busy timeout to 250 milliseconds");

                events::table
                    .filter(events::applied_to_state.eq(true))
                    .order_by(events::event_id.desc())
                    .select(events::event_id)
                    .first::<i64>(conn)
                    .optional()
            })
            .await
            .expect("interaction panicked")
            .expect("failed to fetch the id of the latest applied event");

        // event ids start at 1
        // let (processed_tx, processed_rx) =
        //     watch::channel::<EventId>(EventId(latest_applied_event.unwrap_or(0)));

        let store = DieselSqliteStore {
            pool: pool.clone(),
            //event_tx,
            // processed_rx,
        };

        // tokio::spawn(DieselSqliteStore::handle_payloads(
        //     event_rx,
        //     processed_tx,
        //     pool,
        // ));

        store
    }

    pub async fn interfaces(
        &self,
    ) -> (
        DieselSqliteAccountInterface,
        DieselSqliteAuthInterface,
        DieselSqliteJournalInterface,
        DieselSqliteTransactionInterface,
    ) {
        // we know that the time provider will need to be 'static,
        // so there isn't any point in keeping a reference count
        let time_provider: &DefaultTimeProvider = Box::leak(Box::new(DefaultTimeProvider));

        let auth_interface = DieselSqliteAuthInterface::new(self.clone(), time_provider).await;

        let journal_interface = DieselSqliteJournalInterface::new(self.clone(), time_provider);

        let account_interface = DieselSqliteAccountInterface::new(
            self.clone(),
            journal_interface.clone(),
            time_provider,
        );

        let transaction_interface = DieselSqliteTransactionInterface::new(
            self.clone(),
            journal_interface.clone(),
            time_provider,
        );

        (
            account_interface,
            auth_interface,
            journal_interface,
            transaction_interface,
        )
    }

    async fn validate_entity_type(
        entity_id: Ident,
        entity_type: EntityType,
        conn: &Object,
    ) -> StoreResult<()> {
        let entity: EntityLookup = conn
            .interact(move |conn| {
                entities::table
                    .filter(entities::id.eq(entity_id))
                    .first::<EntityLookup>(conn)
            })
            .await??;

        if entity.entity_type != entity_type {
            Err(StoreError::EntityType {
                expected: entity_type,
                found: entity.entity_type,
            })
        } else {
            Ok(())
        }
    }

    async fn handle_payloads(
        mut event_rx: mpsc::Receiver<(EventId, Box<AnyPayload>, Ident)>,
        processed_tx: watch::Sender<EventId>,
        pool: Pool<Manager<SqliteConnection>>,
    ) -> ! {
        let mut leftover_event: Option<(EventId, Box<AnyPayload>, Ident)> = None;

        loop {
            let (event_id, event_payload, entity_id) = if let Some(ref leftover) = leftover_event {
                leftover.clone()
            } else {
                event_rx.recv().await.expect("event channel closed")
            };

            let conn = pool
                .get()
                .await
                .expect("couldn't get a connection from pool");

            if event_id == *processed_tx.borrow() + 1 {
                conn.interact(move |conn| {
                    conn.transaction(move |tx| {
                        event_payload.execute_sql(entity_id, event_id, tx).map(drop)
                    })
                })
                .await
                .expect("transaction failed")
                .expect("interaction failed");
                processed_tx.send(event_id).expect("all receivers dropped");
            } else {
                // we may be sent an event out of order
                // if this happens, we need to gather all the un-applied events prior to the current one
                // we might as well get future ones while we're at it

                let highest_processed_id = conn
                    .interact(move |conn| {
                        conn.transaction(move |tx| {
                            let pending_events: Vec<_> = events::table
                                .inner_join(entities::table.on(entities::id.eq(events::entity_id)))
                                .filter(events::applied_to_state.eq(false))
                                .order_by(events::event_id.asc())
                                .select((
                                    events::event_id,
                                    events::entity_id,
                                    events::payload,
                                    entities::entity_type,
                                ))
                                .load::<(EventId, Ident, Vec<u8>, EntityType)>(tx)
                                .expect("failed to fetch raw events")
                                .iter()
                                .map(|(event_id, entity_id, payload_bytes, entity_type)| {
                                    let payload = payload_from_bytes(payload_bytes, *entity_type)
                                        .expect("failed to deserialize payload");
                                    (*event_id, *entity_id, payload)
                                })
                                .collect();

                            let last_event_id = pending_events.last().map(|(e_id, _, _)| *e_id);

                            for (event_id, entity_id, payload) in pending_events {
                                payload.execute_sql(entity_id, event_id, tx).map(drop)?;
                            }

                            Ok::<_, diesel::result::Error>(last_event_id)
                        })
                    })
                    .await
                    .expect("transaction failed")
                    .expect("interaction failed");

                if let Some(highest_processed_id) = highest_processed_id
                    && highest_processed_id > *processed_tx.borrow()
                {
                    processed_tx
                        .send(highest_processed_id)
                        .expect("all receivers dropped");
                }

                // clear the queue of already processed events
                let max_id = *processed_tx.borrow();

                loop {
                    match event_rx.try_recv() {
                        // future rebuild requests must be acknowledged explicitly, even if they were handled here
                        Ok((event_id, event_payload, entity_id))
                            if event_id > max_id || event_id < EventId(0) =>
                        {
                            leftover_event = Some((event_id, event_payload, entity_id));
                            break;
                        }
                        Ok(_) => {}
                        Err(TryRecvError::Empty) => {
                            break;
                        }
                        Err(e) => panic!("All senders dropped: {}", e),
                    }
                }
            }
        }
    }

    // async fn wait_for_event_processing(&self, event_id: EventId) -> StoreResult<()> {
    //     self.processed_rx
    //         .clone()
    //         .wait_for(|val| *val >= event_id)
    //         .await?;
    //
    //     Ok(())
    // }
}

impl Debug for DieselSqliteStore {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "DieselSqliteStore")
    }
}

#[derive(Debug, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::sessions)]
pub struct Session {
    pub id: Postcard<Id>,
    pub data: Postcard<HashMap<String, Value>>,
    pub expiry_date: i64,
}

impl From<Record> for Session {
    fn from(record: Record) -> Self {
        Self {
            id: Postcard(record.id),
            data: Postcard(record.data),
            expiry_date: record.expiry_date.unix_timestamp(),
        }
    }
}

impl From<Session> for Record {
    fn from(session: Session) -> Self {
        Self {
            id: session.id.0,
            data: session.data.0,
            // all Sessions are constructed from a Record, guaranteeing that the timestamp is valid
            expiry_date: OffsetDateTime::from_unix_timestamp(session.expiry_date)
                .expect("failed to parse UNIX timestamp"),
        }
    }
}

trait IntoSessionResult<T> {
    fn into_session_result(self) -> session_store::Result<T>;
}

impl<T, E: Error + Display> IntoSessionResult<T> for Result<T, E> {
    fn into_session_result(self) -> session_store::Result<T> {
        self.map_err(|e| Backend(e.to_string()))
    }
}

#[async_trait]
impl SessionStore for DieselSqliteStore {
    async fn create(&self, session_record: &mut Record) -> session_store::Result<()> {
        let conn = self.pool.get().await.into_session_result()?;
        let mut session = Session::from(session_record.clone());

        let session_id = conn
            .interact(move |conn| {
                conn.transaction(move |conn| {
                    loop {
                        match diesel::insert_into(sessions::table)
                            .values(&session)
                            .execute(conn)
                        {
                            Ok(_) => break Ok(session.id.0),
                            // handle a duplicate Id by regenerating it
                            Err(diesel::result::Error::DatabaseError(
                                DatabaseErrorKind::UniqueViolation,
                                _,
                            )) => {
                                session.id.0 = Id::default();
                            }
                            Err(e) => break Err(e),
                        }
                    }
                })
            })
            .await
            .into_session_result()?
            .into_session_result()?;

        session_record.id = session_id;

        Ok(())
    }

    async fn save(&self, session_record: &Record) -> session_store::Result<()> {
        let conn = self.pool.get().await.into_session_result()?;
        let session = Session::from(session_record.clone());

        conn.interact(move |conn| {
            diesel::insert_into(sessions::table)
                .values(&session)
                .on_conflict(sessions::id)
                .do_update()
                .set((
                    sessions::data.eq(excluded(sessions::data)),
                    sessions::expiry_date.eq(excluded(sessions::expiry_date)),
                ))
                .execute(conn)
                .into_session_result()
        })
        .await
        .into_session_result()??;

        Ok(())
    }

    async fn load(&self, session_id: &Id) -> session_store::Result<Option<Record>> {
        let conn = self.pool.get().await.into_session_result()?;
        let session_id = Postcard(*session_id);

        Ok(conn
            .interact(move |conn| {
                sessions::table
                    .filter(sessions::id.eq(&session_id))
                    .first::<Session>(conn)
                    .optional()
                    .into_session_result()
            })
            .await
            .into_session_result()??
            .map(|session| session.into()))
    }

    async fn delete(&self, session_id: &Id) -> session_store::Result<()> {
        let conn = self.pool.get().await.into_session_result()?;
        let session_id = Postcard(*session_id);

        conn.interact(move |conn| {
            diesel::delete(sessions::table.filter(sessions::id.eq(&session_id)))
                .execute(conn)
                .into_session_result()
        })
        .await
        .into_session_result()??;

        Ok(())
    }
}

#[async_trait]
impl ExpiredDeletion for DieselSqliteStore {
    async fn delete_expired(&self) -> session_store::Result<()> {
        let conn = self.pool.get().await.into_session_result()?;

        let now = OffsetDateTime::now_utc().unix_timestamp();

        conn.interact(move |conn| {
            diesel::delete(sessions::table.filter(sessions::expiry_date.lt(now)))
                .execute(conn)
                .into_session_result()
        })
        .await
        .into_session_result()??;

        Ok(())
    }
}

#[derive(Insertable)]
#[diesel(table_name = crate::schema::events)]
pub struct NewEvent {
    pub timestamp: TimeStamp,
    pub authority: Postcard<Authority>,
    pub entity_id: Ident,
    pub payload: Vec<u8>,
    pub applied_to_state: bool,
}

#[derive(Queryable)]
#[diesel(table_name = crate::schema::events)]
struct GenericEvent {
    pub event_id: EventId,
    pub timestamp: TimeStamp,
    pub authority: Postcard<Authority>,
    pub entity_id: Ident,
    pub payload: Vec<u8>,
    pub applied_to_state: bool,
}

#[derive(Debug, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::entities)]
pub struct EntityLookup {
    id: Ident,
    entity_type: EntityType,
}

#[derive(Queryable, QueryableByName)]
struct EventIdRow {
    #[diesel(sql_type = BigInt)]
    event_id: EventId,
}

impl Store for DieselSqliteStore {
    async fn record<I: Entity, T: TimeProvider>(
        &self,
        authority: &Authority,
        time_provider: &T,
        entity_id: I::Id,
        payload: I::Payload,
        when: When,
    ) -> StoreResult<EventId> {
        let conn = self.pool.get().await?;

        let expected_event_id = match when {
            When::Empty => {
                // insert an entity into the table
                // a collision should cause an error
                conn.interact(move |conn| {
                    diesel::insert_into(entities::table)
                        .values(EntityLookup {
                            id: *entity_id,
                            entity_type: I::entity_type(),
                        })
                        .execute(conn)
                })
                .await??;
                EventId(0)
            }
            When::Within(id) => {
                // the entity should already exist in the table
                DieselSqliteStore::validate_entity_type(*entity_id, I::entity_type(), &conn)
                    .await?;
                id
            }
        };

        let timestamp = time_provider.get_time();
        let serialized_payload = postcard::to_allocvec(&payload)?;

        // let cloned_payload = payload.clone();
        let cloned_authority = authority.clone();

        let event_id = conn
            .interact(move |conn| {
                diesel::sql_query(
                    r#"
                    INSERT INTO events (timestamp, authority, entity_id, payload, applied_to_state)
                    SELECT ?, ?, ?, ?, ?
                    WHERE (
                        SELECT COALESCE(MAX(event_id), 0) FROM events WHERE entity_id = ?
                    ) <= ?
                    RETURNING event_id
                    "#,
                )
                .bind::<BigInt, _>(timestamp)
                .bind::<Binary, _>(Postcard(cloned_authority))
                .bind::<Binary, _>(*entity_id)
                .bind::<Binary, _>(serialized_payload)
                .bind::<Bool, _>(false)
                .bind::<Binary, _>(*entity_id)
                .bind::<BigInt, _>(expected_event_id)
                .get_result::<EventIdRow>(conn)
            })
            .await??
            .event_id;

        // the handler will query for side effect events if they exist
        // self.event_tx
        //     .send((event_id, Box::new(cloned_payload.into()), *entity_id))
        //     .await?;

        conn.interact(move |conn| {
            conn.transaction(move |tx| payload.execute_sql(*entity_id, event_id, tx))
        })
        .await??;

        Ok(event_id)
    }

    async fn review<I: Entity>(
        &self,
        entity_id: I::Id,
        after: After,
        page_size: u64,
    ) -> StoreResult<Page<I>> {
        let conn = self.pool.get().await?;

        DieselSqliteStore::validate_entity_type(*entity_id, I::entity_type(), &conn).await?;

        let after_id = match after {
            After::Start => EventId(0),
            After::Id(id) => id,
        };

        let type_erased_id = *entity_id;

        let events: StoreResult<Vec<Event<I>>> = conn
            .interact(move |conn| {
                events::table
                    .filter(events::entity_id.eq(type_erased_id))
                    .filter(events::event_id.ge(after_id))
                    .order_by(events::event_id.asc())
                    .limit(page_size as i64 + 1)
                    .get_results::<GenericEvent>(conn)
            })
            .await??
            .into_iter()
            .map(|ev: GenericEvent| -> StoreResult<Event<I>> {
                Ok(Event {
                    event_id: ev.event_id,
                    timestamp: ev.timestamp,
                    authority: ev.authority,
                    entity_id: I::Id::from(ev.entity_id),
                    payload: postcard::from_bytes(ev.payload.as_slice())?,
                    applied_to_state: ev.applied_to_state,
                })
            })
            .collect();

        let events = events?;

        let last_id = events.last().map(|ev| ev.event_id);

        if events.len() as u64 > page_size
            && let Some(last_id) = last_id
        {
            return Ok(Page {
                events,
                next: Some(last_id + 1),
            });
        }

        Ok(Page { events, next: None })
    }

    async fn get_state<I: Entity>(&self, entity_id: I::Id) -> StoreResult<I::State> {
        let conn = self.pool.get().await?;

        DieselSqliteStore::validate_entity_type(*entity_id, I::entity_type(), &conn).await?;

        conn.interact(move |conn| I::State::fetch(conn, entity_id))
            .await?
    }

    async fn session_store(&self) -> &impl ExpiredDeletion {
        self
    }
}
