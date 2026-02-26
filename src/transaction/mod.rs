pub mod commands;
pub mod service;
pub mod views;

pub use service::TransactionService;

use axum::Router;
use axum::routing::get;
use axum_login::login_required;

pub fn router() -> Router<crate::StateType> {
    Router::new()
        .route(
            "/journal/{id}/transaction",
            get(views::transaction_list_page),
        )
        .route(
            "/journal/{id}/transaction",
            axum::routing::post(commands::transact),
        )
        .route_layer(login_required!(crate::BackendType, login_url = "/signin"))
}

use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;

use crate::authority::Authority;
use crate::authority::UserId;
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::known_errors::KnownErrors;
use crate::known_errors::MonkestoResult;

use crate::event::EventStore;
use chrono::DateTime;
use chrono::Utc;
use dashmap::DashMap;
use serde::Deserialize;
use serde::Serialize;
use sqlx::Decode;
use sqlx::Encode;
use sqlx::Type;
use sqlx::postgres::PgValueRef;

pub trait TransactionStore:
    Clone
    + Send
    + Sync
    + 'static
    + EventStore<Id = TransactionId, Event = TransactionEvent, Error = KnownErrors>
{
    async fn get_journal_transactions(
        &self,
        journal_id: JournalId,
    ) -> MonkestoResult<Vec<TransactionId>>;

    async fn get_transaction(
        &self,
        transaction_id: &TransactionId,
    ) -> MonkestoResult<Option<TransactionState>>;
}

#[derive(Clone)]
pub struct TransactionMemoryStore {
    events: Arc<DashMap<TransactionId, Vec<TransactionEvent>>>,
    transaction_table: Arc<DashMap<TransactionId, TransactionState>>,

    journal_lookup_table: Arc<DashMap<JournalId, Vec<TransactionId>>>,
}

impl TransactionMemoryStore {
    pub fn new() -> Self {
        Self {
            events: Arc::new(DashMap::new()),
            transaction_table: Arc::new(DashMap::new()),
            journal_lookup_table: Arc::new(DashMap::new()),
        }
    }
}

impl EventStore for TransactionMemoryStore {
    type Id = TransactionId;
    type EventId = ();

    type Event = TransactionEvent;
    type Error = KnownErrors;

    async fn record(
        &self,
        id: Self::Id,
        _by: Authority,
        event: Self::Event,
    ) -> Result<Self::EventId, Self::Error> {
        if let TransactionEvent::CreatedTransaction {
            journal_id,
            author,
            updates,
            created_at,
        } = event.clone()
        {
            self.events.insert(id, vec![event]);

            let state = TransactionState {
                id,
                journal_id,
                author,
                updates,
                created_at,
            };

            self.transaction_table.insert(id, state);

            self.journal_lookup_table
                .entry(journal_id)
                .or_default()
                .push(id);

            Ok(())
        } else if let Some(mut events) = self.events.get_mut(&id)
            && let Some(mut state) = self.transaction_table.get_mut(&id)
        {
            state.apply(event.clone());
            events.push(event);
            Ok(())
        } else {
            Err(KnownErrors::InvalidJournal)
        }
    }

    async fn get_events(
        &self,
        id: TransactionId,
        after: usize,
        limit: usize,
    ) -> Result<Vec<TransactionEvent>, Self::Error> {
        let events = self
            .events
            .get(&id)
            .ok_or(KnownErrors::InvalidTransaction { id })?;

        // avoid a panic fn start > len
        if after >= events.len() {
            return Ok(Vec::new());
        }

        // clamp the end value to the vector length
        let end = std::cmp::min(events.len(), after + limit + 1);

        Ok(events[after + 1..end].to_vec())
    }
}

impl TransactionStore for TransactionMemoryStore {
    async fn get_journal_transactions(
        &self,
        journal_id: JournalId,
    ) -> MonkestoResult<Vec<TransactionId>> {
        Ok(self
            .journal_lookup_table
            .get(&journal_id)
            .map(|s| (*s).clone())
            .unwrap_or_default())
    }

    async fn get_transaction(
        &self,
        transaction_id: &TransactionId,
    ) -> MonkestoResult<Option<TransactionState>> {
        Ok(self
            .transaction_table
            .get(transaction_id)
            .map(|s| (*s).clone()))
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Copy)]
pub enum EntryType {
    Debit,
    Credit,
}

impl Display for EntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Debit => write!(f, "Dr"),
            Self::Credit => write!(f, "Cr"),
        }
    }
}

impl FromStr for EntryType {
    type Err = KnownErrors;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Dr" => Ok(Self::Debit),
            "Cr" => Ok(Self::Credit),
            _ => Err(KnownErrors::InvalidInput),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BalanceUpdate {
    pub journal_id: JournalId,
    pub account_id: AccountId,
    pub amount: u64,
    pub entry_type: EntryType,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TransactionEvent {
    CreatedTransaction {
        journal_id: JournalId,
        author: UserId,
        updates: Vec<BalanceUpdate>,
        created_at: DateTime<Utc>,
    },
    UpdatedBalancedUpdates {
        new_balanceupdates: Vec<BalanceUpdate>,
        updater: UserId,
    },
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "smallint")]
#[repr(i16)]
pub enum TransactionEventType {
    Created = 1,
    UpdatedDescription = 2,
    UpdatedBalanceUpdates = 3,
}

impl TransactionEvent {
    #[expect(dead_code)]
    fn get_type(&self) -> TransactionEventType {
        use TransactionEventType::*;
        match self {
            Self::CreatedTransaction { .. } => Created,
            Self::UpdatedBalancedUpdates { .. } => UpdatedBalanceUpdates,
        }
    }
}

impl Type<sqlx::Postgres> for TransactionEvent {
    fn type_info() -> <sqlx::Postgres as sqlx::Database>::TypeInfo {
        <&[u8] as Type<sqlx::Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, sqlx::Postgres> for TransactionEvent {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Postgres as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let bytes: Vec<u8> = postcard::to_allocvec(self)?;
        <&[u8] as Encode<sqlx::Postgres>>::encode(&bytes, buf)
    }
}

impl<'r> Decode<'r, sqlx::Postgres> for TransactionEvent {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let bytes = <&[u8] as Decode<sqlx::Postgres>>::decode(value)?;
        Ok(postcard::from_bytes::<TransactionEvent>(bytes)?)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TransactionState {
    pub id: TransactionId,
    journal_id: JournalId,
    pub author: UserId,
    pub updates: Vec<BalanceUpdate>,
    pub created_at: DateTime<Utc>,
}

impl TransactionState {
    pub fn apply(&mut self, event: TransactionEvent) {
        match event {
            TransactionEvent::CreatedTransaction {
                journal_id,
                author,
                updates,
                created_at,
            } => {
                self.journal_id = journal_id;
                self.author = author;
                self.updates = updates;
                self.created_at = created_at;
            }
            TransactionEvent::UpdatedBalancedUpdates {
                new_balanceupdates, ..
            } => self.updates = new_balanceupdates,
        }
    }
}

#[cfg(test)]
mod test_transaction {
    use super::TransactionEvent;
    use crate::authority::UserId;
    use crate::ident::AccountId;
    use crate::ident::JournalId;
    use crate::transaction::BalanceUpdate;
    use crate::transaction::EntryType;
    use chrono::Utc;
    use sqlx::PgPool;
    use sqlx::prelude::FromRow;

    #[sqlx::test]
    async fn test_encode_decode_transaction_event(pool: PgPool) {
        let original_event = TransactionEvent::CreatedTransaction {
            journal_id: JournalId::new(),
            author: UserId::new(),
            updates: vec![BalanceUpdate {
                journal_id: JournalId::new(),
                account_id: AccountId::new(),
                amount: 45,
                entry_type: EntryType::Debit,
            }],
            created_at: Utc::now(),
        };

        sqlx::query(
            r#"
            CREATE TABLE test_transaction_table (
            event BYTEA
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("failed to create mock transaction table");

        sqlx::query(
            r#"
            INSERT INTO test_transaction_table(
            event
            )
            VALUES ($1)
            "#,
        )
        .bind(&original_event)
        .execute(&pool)
        .await
        .expect("failed to insert transaction into mock table");

        let event: TransactionEvent = sqlx::query_scalar(
            r#"
            SELECT event FROM test_transaction_table
            LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed to fetch transaction from mock table");

        assert_eq!(event, original_event);

        #[derive(FromRow)]
        struct WrapperType {
            event: TransactionEvent,
        }

        let event_wrapper: WrapperType = sqlx::query_as(
            r#"
            SELECT event FROM test_transaction_table
            LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed to fetch transaction from mock table");

        assert_eq!(event_wrapper.event, original_event)
    }
}
