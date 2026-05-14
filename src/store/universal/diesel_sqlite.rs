use crate::auth::user::UserState;
use crate::authority::{Authority, UserId};
use crate::ident::Ident;
use crate::journal::JournalId;
use crate::postcard::Postcard;
use crate::schema::sessions;
use crate::schema::users;
use crate::store::universal::registry::AnyPayload;
use crate::store::universal::{
    Entity, Event, EventId, GetPayloadUsage, PayloadUsage, SequenceId, Store, StoreResult,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use deadpool_diesel::Runtime;
use deadpool_diesel::{Manager, Pool};
use diesel::result::DatabaseErrorKind;
use diesel::upsert::excluded;
use diesel::{Connection, QueryDsl, RunQueryDsl};
use diesel::{ExpressionMethods, OptionalExtension};
use diesel::{Insertable, Queryable, Selectable, SqliteConnection};
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use tokio::sync::{mpsc, watch};
use tower_sessions::cookie::time::OffsetDateTime;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::Error::Backend;
use tower_sessions::{ExpiredDeletion, SessionStore, session_store};

#[derive(Debug, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::journal_members_lookup)]
pub struct JournalMembersLookup {
    user_id: UserId,
    journal_id: JournalId,
}

#[derive(Clone)]
pub struct DieselSqliteStore {
    pool: Pool<Manager<SqliteConnection>>,
    sender: mpsc::Sender<(EventId, Box<AnyPayload>, Ident)>,
    processed_rx: watch::Receiver<EventId>,
}

impl DieselSqliteStore {
    pub fn new(url: &str) -> DieselSqliteStore {
        let manager = Manager::new(url, Runtime::Tokio1);

        let pool = Pool::builder(manager)
            .max_size(16)
            .build()
            .expect("failed to initialize Diesel connection pool");

        let (event_tx, event_rx) = mpsc::channel::<(EventId, Box<AnyPayload>, Ident)>(64);

        let (processed_tx, processed_rx) = watch::channel::<EventId>(EventId(0));

        let store = DieselSqliteStore {
            pool: pool.clone(),
            sender: event_tx,
            processed_rx,
        };

        tokio::spawn(DieselSqliteStore::handle_payloads(
            event_rx,
            processed_tx,
            pool,
        ));

        store
    }

    // TODO: avoid unnecessary match statements using the type system
    // TODO: finish this function
    #[allow(unused)]
    async fn handle_payloads(
        mut event_rx: mpsc::Receiver<(EventId, Box<AnyPayload>, Ident)>,
        processed_tx: watch::Sender<EventId>,
        pool: Pool<Manager<SqliteConnection>>,
    ) -> ! {
        let conn = pool
            .get()
            .await
            .expect("couldn't get a connection from pool");

        loop {
            // the sender should have a static lifetime; it being dropped is an unrecoverable error
            let (event_id, payload, entity_id) = event_rx.recv().await.expect("couldn't get event");

            conn.interact(move |conn| match *payload.clone() {
                AnyPayload::User(user_payload) => {
                    match user_payload.usage(entity_id) {
                        PayloadUsage::CreatesState(state) => {}
                        PayloadUsage::ModifiesState(user_payload) => {}
                    }
                    let user = users::dsl::users
                        .filter(users::id.eq(&entity_id))
                        .first::<UserState>(conn)
                        .expect("user state doesn't exist");
                }
                AnyPayload::Passkey(passkey_payload) => {}

                _ => {}
            })
            .await
            .expect("interaction panicked");

            // the receiver being dropped is an unrecoverable error
            processed_tx.send(event_id).expect("all receivers dropped");
        }
    }

    async fn wait_for_event_processing(&self, event_id: EventId) {
        let mut processed_rx = self.processed_rx.clone();

        // the broadcast channel closing is an unrecoverable error
        processed_rx
            .wait_for(|val| *val >= event_id)
            .await
            .expect("broadcast channel closed");
    }
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
                        match diesel::insert_into(sessions::dsl::sessions)
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
            diesel::insert_into(sessions::dsl::sessions)
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
                sessions::dsl::sessions
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
            diesel::delete(sessions::dsl::sessions.filter(sessions::id.eq(&session_id)))
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
            diesel::delete(sessions::dsl::sessions.filter(sessions::expiry_date.lt(now)))
                .execute(conn)
                .into_session_result()
        })
        .await
        .into_session_result()??;

        Ok(())
    }
}

#[expect(unused)]
impl Store for DieselSqliteStore {
    async fn record<I: Entity>(
        &self,
        authority: Authority,
        at: DateTime<Utc>,
        entity_id: I::Id,
        payload: I::Payload,
        expected_sequence: SequenceId,
    ) -> StoreResult<EventId> {
        todo!()
    }

    async fn replay_events<I: Entity>(
        &self,
        entity_id: I::Id,
        starting_sequence: SequenceId,
    ) -> Vec<Event<I>> {
        todo!()
    }

    async fn get_state<I: Entity>(&self, entity_id: I::Id) -> StoreResult<(I::State, SequenceId)> {
        todo!()
    }

    async fn rebuild_state<I: Entity>(
        &self,
        entity_id: I::Id,
        events: Vec<Event<I>>,
    ) -> StoreResult<()> {
        todo!()
    }

    async fn session_store(&self) -> &impl ExpiredDeletion {
        self
    }
}
