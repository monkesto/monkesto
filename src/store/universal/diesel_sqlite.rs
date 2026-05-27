use crate::account::AccountState;
use crate::auth::passkey::PasskeyState;
use crate::auth::user::UserState;
use crate::authority::{Authority, UserId};
use crate::ident::Ident;
use crate::journal::{JournalId, JournalModifiedPayload, JournalPayload, JournalState};
use crate::postcard::Postcard;
use crate::schema::sessions;
use crate::schema::{accounts, events, journal_members_lookup};
use crate::schema::{entities, examples, journals, passkeys, transactions, users};
use crate::store::universal::error::StoreError;
use crate::store::universal::example_entity::{ExamplePayload, ExampleState};
use crate::store::universal::registry::{AnyPayload, EntityType, payload_from_bytes};
use crate::store::universal::time_provider::TimeProvider;
use crate::store::universal::{
    After, Entity, Event, EventId, FetchState, GetPayloadUsage, PayloadUsage, Store, StoreResult,
    TimeStamp, When,
};
use crate::transaction::{
    EntryType, TransactionModifiedPayload, TransactionPayload, TransactionState,
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
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{mpsc, watch};
use tower_sessions::cookie::time::OffsetDateTime;
use tower_sessions::session::{Id, Record};
use tower_sessions::session_store::Error::Backend;
use tower_sessions::{ExpiredDeletion, SessionStore, session_store};

macro_rules! create_or_update_state {
    ($conn:ident, $event_id: ident, $entity_id:ident, $payload:ident, $entity_type:ident, $( $variant:path => $state_type:path as $table_name:ident),* $(,)?) => {
        match $payload.clone() {
            $(
                $variant(variant_payload) => {
                    match variant_payload.usage($entity_id, $event_id) {
                        PayloadUsage::CreatesState(state) => {
                            diesel::insert_into($crate::schema::$table_name::table).values(&state).execute($conn)?;

                            diesel::insert_into($crate::schema::entities::table).values(
                                EntityLookup {
                                    id: $entity_id,
                                    entity_type: $entity_type
                                }
                            ).execute($conn)?;
                        }
                        PayloadUsage::ModifiesState(mod_fn) => {
                            let mut state = $crate::schema::$table_name::table.filter($crate::schema::$table_name::id.eq(&$entity_id)).first::<$state_type>($conn)?;
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
    event_tx: mpsc::Sender<(EventId, Box<AnyPayload>, Ident, EntityType)>,
    processed_rx: watch::Receiver<EventId>,
    processed_rebuilds_rx: watch::Receiver<EventId>,
    rebuild_num: Arc<AtomicI64>,
}

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations/");

impl DieselSqliteStore {
    pub async fn new(url: &str) -> DieselSqliteStore {
        let manager = Manager::new(url, Runtime::Tokio1);

        let pool = Pool::builder(manager)
            .max_size(16)
            .build()
            .expect("failed to initialize Diesel connection pool");

        let (event_tx, event_rx) =
            mpsc::channel::<(EventId, Box<AnyPayload>, Ident, EntityType)>(64);

        let conn: Object = pool
            .get()
            .await
            .expect("couldn't get a connection from pool");

        let latest_applied_event = conn
            .interact(|conn| {
                conn.run_pending_migrations(MIGRATIONS)
                    .expect("failed to run migrations");
                conn.batch_execute("PRAGMA journal_mode = WAL;")
                    .expect("failed to enable WAL mode");
                conn.batch_execute("PRAGMA synchronous = NORMAL;")
                    .expect("failed to set synchronous mode");
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
        entity_id: Ident,
        payload: AnyPayload,
        entity_type: EntityType,
    ) -> diesel::QueryResult<()> {
        conn.transaction(|tx| {
            let payload_clone = payload.clone();

            create_or_update_state!(
                tx, event_id,entity_id, payload, entity_type,
                AnyPayload::User => UserState as users,
                AnyPayload::Passkey => PasskeyState as passkeys,
                AnyPayload::Account => AccountState as accounts,
                AnyPayload::Journal => JournalState as journals,
                AnyPayload::Transaction => TransactionState as transactions,
                AnyPayload::Example => ExampleState as examples
            );

            diesel::update(events::table)
                .filter(events::event_id.eq(event_id))
                .set(events::applied_to_state.eq(true))
                .execute(tx)?;

            // handle special cases
            match payload_clone {
                AnyPayload::Transaction(transaction_payload) => {
                    match transaction_payload {
                        TransactionPayload::Created { updates, .. } => {
                            for update in updates {
                                diesel::update(accounts::table)
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
                            let old_payload = events::table
                                .filter(events::entity_id.eq(entity_id))
                                .order_by(events::event_id.desc())
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
                                        diesel::update(accounts::table)
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
                                        diesel::update(accounts::table)
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
                                        diesel::update(accounts::table)
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
                            diesel::insert_into(journal_members_lookup::table)
                                .values(JournalMembersLookup {
                                    user_id: id,
                                    journal_id: entity_id.into(),
                                })
                                .execute(tx)?;
                        }
                        JournalModifiedPayload::RemovedTenant { id, .. } => {
                            diesel::delete(journal_members_lookup::table)
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

    async fn validate_entity_type<I: Entity>(entity_id: I::Id, conn: &Object) -> StoreResult<()> {
        let id = *entity_id;

        let entity: EntityLookup = conn
            .interact(move |conn| {
                entities::table
                    .filter(entities::id.eq(id))
                    .first::<EntityLookup>(conn)
            })
            .await??;

        if entity.entity_type != I::entity_type() {
            Err(StoreError::EntityType {
                expected: I::entity_type(),
                found: entity.entity_type,
            })
        } else {
            Ok(())
        }
    }

    async fn handle_payloads(
        mut event_rx: mpsc::Receiver<(EventId, Box<AnyPayload>, Ident, EntityType)>,
        processed_tx: watch::Sender<EventId>,
        processed_rebuilds_tx: watch::Sender<EventId>,
        pool: Pool<Manager<SqliteConnection>>,
    ) -> ! {
        let mut leftover_event: Option<(EventId, Box<AnyPayload>, Ident, EntityType)> = None;

        loop {
            let (event_id, event_payload, entity_id, entity_type) =
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
                        entity_id,
                        *event_payload,
                        entity_type,
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
                            .load::<(EventId, Ident, Vec<u8>, EntityType)>(conn)
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
                            DieselSqliteStore::apply_event(
                                conn,
                                event_id,
                                entity_id,
                                payload,
                                entity_type,
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
                        Ok((event_id, event_payload, entity_id, entity_type))
                            if event_id > max_id || event_id < EventId(0) =>
                        {
                            leftover_event =
                                Some((event_id, event_payload, entity_id, entity_type));
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

    async fn wait_for_event_processing(&self, event_id: EventId) -> StoreResult<()> {
        self.processed_rx
            .clone()
            .wait_for(|val| *val >= event_id)
            .await?;

        Ok(())
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

#[derive(QueryableByName)]
struct EventIdRow {
    #[diesel(sql_type = BigInt)]
    event_id: EventId,
}

impl Store for DieselSqliteStore {
    async fn record<I: Entity, T: TimeProvider>(
        &self,
        authority: Authority,
        time_provider: &T,
        entity_id: I::Id,
        payload: I::Payload,
        when: When,
    ) -> StoreResult<EventId> {
        let conn = self.pool.get().await?;

        // if the payload creates an entity, it shouldn't be in the entity table
        // if it is in the table already, the sequence id should prevent the creation of multiple entities with the same id

        DieselSqliteStore::validate_entity_type::<I>(entity_id, &conn).await?;

        let timestamp = time_provider.get_time();
        let serialized_payload = postcard::to_allocvec(&payload)?;
        let expected_event_id = match when {
            When::Empty => EventId(0),
            When::Within(id) => id,
        };

        let event_id: EventId = conn
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
                .bind::<Binary, _>(Postcard(authority))
                .bind::<Binary, _>(*entity_id)
                .bind::<Binary, _>(serialized_payload)
                .bind::<Bool, _>(false)
                .bind::<Binary, _>(*entity_id)
                .bind::<BigInt, _>(expected_event_id)
                .get_result::<EventIdRow>(conn)
            })
            .await??
            .event_id;

        self.event_tx
            .send((
                event_id,
                Box::new(payload.into()),
                *entity_id,
                I::entity_type(),
            ))
            .await?;

        Ok(event_id)
    }

    async fn replay_events<I: Entity>(
        &self,
        entity_id: I::Id,
        after: After,
    ) -> StoreResult<Vec<Event<I>>> {
        let conn = self.pool.get().await?;

        DieselSqliteStore::validate_entity_type::<I>(entity_id, &conn).await?;

        let after_id = match after {
            After::Start => EventId(0),
            After::Id(id) => id,
        };

        let type_erased_id = *entity_id;

        let events: StoreResult<Vec<Event<I>>> = conn
            .interact(move |conn| {
                events::table
                    .filter(events::entity_id.eq(type_erased_id))
                    .filter(events::event_id.gt(after_id))
                    .order_by(events::event_id.asc())
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

        events
    }

    async fn get_state<I: Entity>(&self, entity_id: I::Id) -> StoreResult<I::State> {
        let conn = self.pool.get().await?;

        DieselSqliteStore::validate_entity_type::<I>(entity_id, &conn).await?;

        conn.interact(move |conn| I::State::fetch(conn, entity_id))
            .await?
    }

    async fn rebuild_state<I: Entity>(&self, entity_id: I::Id) -> StoreResult<()> {
        let conn = self.pool.get().await?;

        DieselSqliteStore::validate_entity_type::<I>(entity_id, &conn).await?;

        let type_erased_id = *entity_id;

        conn.interact(move |conn| {
            conn.transaction(|tx| {
                diesel::update(events::table)
                    .filter(events::entity_id.eq(type_erased_id))
                    .set(events::applied_to_state.eq(false))
                    .execute(tx)?;

                match I::entity_type() {
                    EntityType::Example => {
                        diesel::delete(examples::table)
                            .filter(examples::id.eq(type_erased_id))
                            .execute(tx)?;
                    }
                    EntityType::Journal => {
                        diesel::delete(journals::table)
                            .filter(journals::id.eq(type_erased_id))
                            .execute(tx)?;

                        diesel::delete(journal_members_lookup::table)
                            .filter(journal_members_lookup::journal_id.eq(type_erased_id))
                            .execute(tx)?;
                    }
                    EntityType::Account => {
                        diesel::delete(accounts::table)
                            .filter(accounts::id.eq(type_erased_id))
                            .execute(tx)?;
                    }
                    EntityType::Transaction => {
                        diesel::delete(transactions::table)
                            .filter(transactions::id.eq(type_erased_id))
                            .execute(tx)?;

                        // we also have undo the balance updates associated with the transaction
                        let old_payload: TransactionPayload = events::table
                            .filter(events::entity_id.eq(type_erased_id))
                            .order_by(events::event_id.desc())
                            .select(events::payload)
                            .first::<TransactionPayload>(tx)?;

                        let old_updates = match old_payload {
                            TransactionPayload::Created { updates, .. } => Some(updates),
                            TransactionPayload::Modified(
                                TransactionModifiedPayload::UpdatedBalancedUpdates {
                                    new_balanceupdates,
                                },
                            ) => Some(new_balanceupdates),
                            TransactionPayload::Modified(TransactionModifiedPayload::Deleted) => {
                                None
                            }
                        };

                        if let Some(old_updates) = old_updates {
                            for old_update in old_updates {
                                diesel::update(accounts::table)
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
                    EntityType::Passkey => {
                        diesel::delete(passkeys::table)
                            .filter(passkeys::id.eq(type_erased_id))
                            .execute(tx)?;
                    }
                    EntityType::User => {
                        diesel::delete(users::table)
                            .filter(users::id.eq(type_erased_id))
                            .execute(tx)?;
                    }
                    EntityType::Grant | EntityType::Role => {
                        todo!("grant and role don't have associated tables")
                    }
                }

                Ok::<_, diesel::result::Error>(())
            })
        })
        .await??;

        // send an impossible EventId to the handler to trigger a query
        let rebuild_id = EventId(self.rebuild_num.fetch_sub(1, Ordering::SeqCst));

        // the event values don't matter; the handler will discard them when it gets an unexpected id
        self.event_tx
            .send((
                rebuild_id,
                Box::new(AnyPayload::Example(ExamplePayload::Created)),
                Ident::new10(),
                EntityType::Example,
            ))
            .await?;

        // wait for the handler to process the rebuild
        self.processed_rebuilds_rx
            .clone()
            .wait_for(|event_id| *event_id >= rebuild_id)
            .await?;

        Ok(())
    }

    async fn session_store(&self) -> &impl ExpiredDeletion {
        self
    }
}
