use crate::account::AccountState;
use crate::auth::passkey::PasskeyState;
use crate::auth::user::UserState;
use crate::authority::{Authority, UserId};
use crate::ident::Ident;
use crate::journal::{JournalId, JournalModifiedPayload, JournalPayload, JournalState};
use crate::postcard::Postcard;
use crate::schema::sessions;
use crate::schema::{accounts, events, journal_members_lookup};
use crate::store::universal::example_entity::ExampleState;
use crate::store::universal::registry::{AnyPayload, EntityType};
use crate::store::universal::time_provider::TimeProvider;
use crate::store::universal::{
    Entity, Event, EventId, GetPayloadUsage, Payload, PayloadUsage, SequenceId, Store, StoreResult,
    TimeStamp, payload_from_bytes,
};
use crate::transaction::{
    EntryType, TransactionModifiedPayload, TransactionPayload, TransactionState,
};
use async_trait::async_trait;
use deadpool_diesel::Runtime;
use deadpool_diesel::sqlite::Object;
use deadpool_diesel::{Manager, Pool};
use diesel::result::DatabaseErrorKind;
use diesel::upsert::excluded;
use diesel::{Connection, QueryDsl, QueryableByName, RunQueryDsl};
use diesel::{ExpressionMethods, OptionalExtension};
use diesel::{Insertable, Queryable, Selectable, SqliteConnection};
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter};
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{mpsc, watch};
use tower_sessions::cookie::time::OffsetDateTime;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::Error::Backend;
use tower_sessions::{ExpiredDeletion, SessionStore, session_store};

macro_rules! create_or_update_state {
    ($conn:ident, $event_id: ident, $sequence_id: ident, $entity_id:ident, $payload:ident, $( $variant:path => $state_type:path as $table_name:ident),* $(,)?) => {
        match $payload.clone() {
            $(
                $variant(variant_payload) => {
                    match variant_payload.usage($entity_id, $sequence_id) {
                        PayloadUsage::CreatesState(state) => {
                            diesel::insert_into($crate::schema::$table_name::dsl::$table_name).values(&state).execute($conn)?;
                        }
                        PayloadUsage::ModifiesState(mod_fn) => {
                            let mut state = $crate::schema::$table_name::dsl::$table_name.filter($crate::schema::$table_name::id.eq(&$entity_id)).first::<$state_type>($conn)?;
                            mod_fn(&mut state);

                            diesel::update($crate::schema::$table_name::table.filter($crate::schema::$table_name::id.eq(&$entity_id))).set(&state).execute($conn)?;
                        }
                    }
                },
            )*
        }
    };
}

#[derive(Debug, Queryable, Selectable, Insertable)]
#[diesel(table_name = crate::schema::journal_members_lookup)]
pub struct JournalMembersLookup {
    user_id: UserId,
    journal_id: JournalId,
}

#[derive(Clone)]
pub struct DieselSqliteStore {
    pool: Pool<Manager<SqliteConnection>>,
    event_tx: mpsc::Sender<(EventId, SequenceId, Box<AnyPayload>, Ident)>,
    processed_rx: watch::Receiver<EventId>,
    processed_rebuilds_rx: watch::Receiver<EventId>,
    rebuild_num: Arc<AtomicI64>,
}

impl DieselSqliteStore {
    pub async fn new(url: &str) -> DieselSqliteStore {
        let manager = Manager::new(url, Runtime::Tokio1);

        let pool = Pool::builder(manager)
            .max_size(16)
            .build()
            .expect("failed to initialize Diesel connection pool");

        let (event_tx, event_rx) =
            mpsc::channel::<(EventId, SequenceId, Box<AnyPayload>, Ident)>(64);

        let conn: Object = pool
            .get()
            .await
            .expect("couldn't get a connection from pool");

        let latest_applied_event = conn
            .interact(|conn| {
                events::dsl::events
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
        let (processed_tx, processed_rx) =
            watch::channel::<EventId>(EventId(latest_applied_event.unwrap_or(0)));

        let (processed_rebuilds_tx, processed_rebuilds_rx) = watch::channel::<EventId>(EventId(-1));

        let store = DieselSqliteStore {
            pool: pool.clone(),
            event_tx,
            processed_rx,
            processed_rebuilds_rx,
            rebuild_num: Arc::new(AtomicI64::new(-1)),
        };

        tokio::spawn(DieselSqliteStore::handle_payloads(
            event_rx,
            processed_tx,
            processed_rebuilds_tx,
            pool,
        ));

        store
    }

    fn apply_event(
        conn: &mut SqliteConnection,
        event_id: EventId,
        sequence_id: SequenceId,
        entity_id: Ident,
        payload: AnyPayload,
    ) -> diesel::QueryResult<()> {
        conn.transaction(|tx| {
            let payload_clone = payload.clone();

            create_or_update_state!(
                tx, event_id, sequence_id ,entity_id, payload,
                AnyPayload::User => UserState as users,
                AnyPayload::Passkey => PasskeyState as passkeys,
                AnyPayload::Account => AccountState as accounts,
                AnyPayload::Journal => JournalState as journals,
                AnyPayload::Transaction => TransactionState as transactions,
                AnyPayload::Example => ExampleState as examples
            );

            diesel::update(events::dsl::events)
                .filter(events::event_id.eq(event_id))
                .set(events::applied_to_state.eq(true))
                .execute(tx)?;

            // handle special cases
            match payload_clone {
                AnyPayload::Transaction(transaction_payload) => {
                    match transaction_payload {
                        TransactionPayload::Created { updates, .. } => {
                            for update in updates {
                                diesel::update(accounts::dsl::accounts)
                                    .filter(accounts::id.eq(update.account_id))
                                    .set(accounts::balance.eq(accounts::balance
                                        + if update.entry_type == EntryType::Credit {
                                            update.amount as i64
                                        } else {
                                            -(update.amount as i64)
                                        }))
                                    .execute(tx)?;
                            }
                        }
                        TransactionPayload::Modified(modified_payload) => {
                            let old_payload = events::dsl::events
                                .filter(events::entity_id.eq(entity_id))
                                .order_by(events::sequence_id.asc())
                                .select(events::payload)
                                .first::<TransactionPayload>(tx)?;

                            let old_updates = match old_payload {
                                TransactionPayload::Created { updates, .. } => updates,
                                TransactionPayload::Modified(
                                    TransactionModifiedPayload::UpdatedBalancedUpdates {
                                        new_balanceupdates,
                                    },
                                ) => new_balanceupdates,
                                TransactionPayload::Modified(
                                    TransactionModifiedPayload::Deleted,
                                ) => unreachable!(
                                    "it should be impossible to modify a deleted transaction"
                                ),
                            };

                            match modified_payload {
                                TransactionModifiedPayload::UpdatedBalancedUpdates {
                                    new_balanceupdates,
                                } => {
                                    // undo old updates
                                    for old_update in old_updates {
                                        diesel::update(accounts::dsl::accounts)
                                            .filter(accounts::id.eq(old_update.account_id))
                                            .set(accounts::balance.eq(accounts::balance
                                                + if old_update.entry_type != EntryType::Credit {
                                                    old_update.amount as i64
                                                } else {
                                                    -(old_update.amount as i64)
                                                }))
                                            .execute(tx)?;
                                    }

                                    // apply new updates
                                    for update in new_balanceupdates {
                                        diesel::update(accounts::dsl::accounts)
                                            .filter(accounts::id.eq(update.account_id))
                                            .set(accounts::balance.eq(accounts::balance
                                                + if update.entry_type == EntryType::Credit {
                                                    update.amount as i64
                                                } else {
                                                    -(update.amount as i64)
                                                }))
                                            .execute(tx)?;
                                    }
                                }
                                TransactionModifiedPayload::Deleted => {
                                    // undo old updates
                                    for old_update in old_updates {
                                        diesel::update(accounts::dsl::accounts)
                                            .filter(accounts::id.eq(old_update.account_id))
                                            .set(accounts::balance.eq(accounts::balance
                                                + if old_update.entry_type != EntryType::Credit {
                                                    old_update.amount as i64
                                                } else {
                                                    -(old_update.amount as i64)
                                                }))
                                            .execute(tx)?;
                                    }
                                }
                            }
                        }
                    }
                }

                AnyPayload::Journal(JournalPayload::Modified(journal_payload)) => {
                    match journal_payload {
                        JournalModifiedPayload::AddedTenant { id, .. } => {
                            diesel::insert_into(
                                journal_members_lookup::dsl::journal_members_lookup,
                            )
                            .values(JournalMembersLookup {
                                user_id: id,
                                journal_id: entity_id.into(),
                            })
                            .execute(tx)?;
                        }
                        JournalModifiedPayload::RemovedTenant { id, .. } => {
                            diesel::delete(journal_members_lookup::dsl::journal_members_lookup)
                                .filter(journal_members_lookup::user_id.eq(&id))
                                .filter(journal_members_lookup::journal_id.eq(entity_id))
                                .execute(tx)?;
                        }
                        _ => {}
                    }
                }

                _ => {}
            }

            Ok(())
        })
    }

    async fn handle_payloads(
        mut event_rx: mpsc::Receiver<(EventId, SequenceId, Box<AnyPayload>, Ident)>,
        processed_tx: watch::Sender<EventId>,
        processed_rebuilds_tx: watch::Sender<EventId>,
        pool: Pool<Manager<SqliteConnection>>,
    ) -> ! {
        let mut leftover_event: Option<(EventId, SequenceId, Box<AnyPayload>, Ident)> = None;

        loop {
            let (event_id, sequence_id, event_payload, entity_id) =
                if let Some(ref leftover) = leftover_event {
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
                    DieselSqliteStore::apply_event(
                        conn,
                        event_id,
                        sequence_id,
                        entity_id,
                        *event_payload,
                    )
                    .expect("failed to apply event");
                })
                .await
                .expect("interaction failed");
                processed_tx.send(event_id).expect("all receivers dropped");
            } else {
                // we may be sent an event out of order
                // if this happens, we need to gather all the un-applied events prior to the current one
                // we might as well get future ones while we're at it

                let highest_processed_id = conn
                    .interact(move |conn| {
                        let un_applied_events: Vec<_> = events::dsl::events
                            .filter(events::applied_to_state.eq(false))
                            .order_by(events::event_id.asc())
                            .select((
                                events::event_id,
                                events::sequence_id,
                                events::entity_id,
                                events::entity_type,
                                events::payload,
                            ))
                            .load::<(EventId, SequenceId, Ident, EntityType, Vec<u8>)>(conn)
                            .expect("failed to fetch raw events")
                            .iter()
                            .map(
                                |(event_id, sequence_id, entity_id, entity_type, payload_bytes)| {
                                    let payload = payload_from_bytes(payload_bytes, *entity_type)
                                        .expect("failed to deserialize payload");
                                    (*event_id, *sequence_id, *entity_id, payload)
                                },
                            )
                            .collect();

                        let last_event_id = un_applied_events.last().map(|(e_id, _, _, _)| *e_id);

                        for (event_id, sequence_id, entity_id, payload) in un_applied_events {
                            DieselSqliteStore::apply_event(
                                conn,
                                event_id,
                                sequence_id,
                                entity_id,
                                payload,
                            )
                            .expect("failed to apply event");
                        }

                        last_event_id
                    })
                    .await
                    .expect("interaction failed");

                if let Some(highest_processed_id) = highest_processed_id
                    && highest_processed_id > *processed_tx.borrow()
                {
                    processed_tx
                        .send(highest_processed_id)
                        .expect("all receivers dropped");
                }

                // signal to the rebuild requester that we are done
                if event_id < EventId(0) {
                    processed_rebuilds_tx
                        .send(event_id)
                        .expect("all receivers dropped");
                }

                // clear the queue of already processed events
                let max_id = *processed_tx.borrow();

                loop {
                    match event_rx.try_recv() {
                        // future rebuild requests must be acknowledged explicitly, even if they were handled here
                        Ok((event_id, sequence_id, event_payload, entity_id))
                            if event_id > max_id || event_id < EventId(0) =>
                        {
                            leftover_event =
                                Some((event_id, sequence_id, event_payload, entity_id));
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

#[derive(Insertable)]
#[diesel(table_name = crate::schema::events)]
pub struct NewEvent {
    pub sequence_id: SequenceId,
    pub timestamp: TimeStamp,
    pub authority: Postcard<Authority>,
    pub entity_id: Ident,
    pub payload: Vec<u8>,
    pub entity_type: EntityType,
    pub applied_to_state: bool,
}

#[derive(Queryable)]
#[diesel(table_name = crate::schema::events)]
struct TypeErasedEvent {
    pub event_id: EventId,
    pub sequence_id: SequenceId,
    pub timestamp: TimeStamp,
    pub authority: Postcard<Authority>,
    pub entity_id: Ident,
    pub payload: Vec<u8>,
    pub entity_type: EntityType,
    pub applied_to_state: bool,
}

#[expect(unused)]
impl Store for DieselSqliteStore {
    async fn record<I: Entity, T: TimeProvider>(
        &self,
        authority: Authority,
        time_provider: &T,
        entity_id: I::Id,
        payload: I::Payload,
        expected_sequence: SequenceId,
    ) -> StoreResult<EventId> {
        // TODO: query an entity table to ensure that the entity's type matches the generic

        let new_event = NewEvent {
            sequence_id: expected_sequence,
            timestamp: time_provider.get_time(),
            authority: Postcard(authority),
            entity_id: *entity_id,
            payload: postcard::to_allocvec(&payload)?,
            entity_type: I::entity_type(),
            applied_to_state: false,
        };

        let conn = self.pool.get().await?;

        let event_id: EventId = conn
            .interact(move |conn| {
                Ok::<_, diesel::result::Error>(
                    diesel::insert_into(events::dsl::events)
                        .values(new_event)
                        .returning(events::event_id)
                        .get_result(conn)?,
                )
            })
            .await??;

        self.event_tx
            .send((
                event_id,
                expected_sequence,
                Box::new(payload.into()),
                *entity_id,
            ))
            .await?;

        Ok(event_id)
    }

    async fn replay_events<I: Entity>(
        &self,
        entity_id: I::Id,
        starting_sequence: SequenceId,
    ) -> StoreResult<Vec<Event<I>>> {
        let conn = self.pool.get().await?;

        let type_erased_id = *entity_id;

        let raw_events: Vec<TypeErasedEvent> = conn
            .interact(move |conn| {
                Ok::<_, diesel::result::Error>(
                    events::dsl::events
                        .filter(events::entity_id.eq(type_erased_id))
                        .filter(events::sequence_id.ge(starting_sequence))
                        .order_by(events::sequence_id.asc())
                        .get_results::<TypeErasedEvent>(conn)?,
                )
            })
            .await??;

        todo!()
    }

    async fn get_state<I: Entity>(&self, entity_id: I::Id) -> StoreResult<(I::State, SequenceId)> {
        todo!()
    }

    async fn rebuild_state<I: Entity>(&self, entity_id: I::Id) -> StoreResult<()> {
        todo!()
    }

    async fn session_store(&self) -> &impl ExpiredDeletion {
        self
    }
}
