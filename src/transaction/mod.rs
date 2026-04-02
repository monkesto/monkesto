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

use std::fmt::Debug;
use std::fmt::Display;
use std::ops::Deref;
use std::str::FromStr;
use std::sync::Arc;

use crate::authority::Authority;
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::ident::TransactionId;

use crate::account::AccountStoreError;
use crate::event::Event;
use crate::event::EventStore;
use crate::journal::JournalStoreError;
use crate::journal::Permissions;
use crate::transaction::TransactionStoreError::InvalidEntryType;
use TransactionStoreError::InvalidTransaction;
use dashmap::DashMap;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde::Serialize;
use sqlx::Decode;
use sqlx::Encode;
use sqlx::Type;
use sqlx::postgres::PgValueRef;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Debug, Error, Serialize, Deserialize, Eq, PartialEq)]
pub enum TransactionStoreError {
    #[error("Invalid transaction: {0}")]
    InvalidTransaction(TransactionId),

    #[error("Invalid journal: {0}")]
    InvalidJournal(JournalId),

    #[error("Invalid account: {0}")]
    InvalidAccount(AccountId),

    #[error("Invalid entry type: {0} expected \"Dr\" or \"Cr\"")]
    InvalidEntryType(String),

    #[error("A journal id was not given for one of the transaction updates")]
    JournalIdNotSupplied,

    #[error("An amount was not given for one of the transaction updates")]
    AmountNotSupplied,

    #[error("An entry type was not given for one of the transaction updates")]
    EntryTypeNotSupplied,

    #[error("Failed to parse a decimal number from the input: {0}")]
    ParseDecimal(String),

    #[error("A partial cent value was supplied in the number: {0}")]
    PartialCentValue(Decimal),

    #[error(
        "The balance update {0} is too large! The number of cents should fit in a 64 bit integer."
    )]
    BalanceUpdateTooLarge(Decimal),

    #[error("The balance update {0} is a negative number! Please use the Dr/Cr selector instead.")]
    NegativeBalanceUpdate(Decimal),

    #[error("The transaction does not have balanced credits and debits")]
    TransactionNotBalanced,

    #[error("No balance updates were supplied in the transaction")]
    NoBalanceUpdatesSupplied,

    #[error("The journal store returned an error: {0}")]
    JournalStore(#[from] JournalStoreError),

    #[error("The account store returned an error: {0}")]
    AccountStore(#[from] AccountStoreError),

    #[error("The user does not have the required permission: {:?}", .0)]
    PermissionError(Permissions),
}

pub type TransactionStoreResult<T> = Result<T, TransactionStoreError>;

pub trait TransactionStore:
    Clone
    + Send
    + Sync
    + 'static
    + EventStore<Id = TransactionId, Payload = TransactionPayload, Error = TransactionStoreError>
{
    async fn get_journal_transactions(
        &self,
        journal_id: JournalId,
    ) -> TransactionStoreResult<Vec<TransactionId>>;

    async fn get_transaction(
        &self,
        transaction_id: &TransactionId,
    ) -> TransactionStoreResult<Option<TransactionState>>;

    async fn get_transaction_authority(
        &self,
        transaction_id: &TransactionId,
    ) -> TransactionStoreResult<Authority>;
}

#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct TransactionMemoryStore {
    global_events: Arc<Mutex<Vec<Arc<Event<TransactionPayload, TransactionId>>>>>,
    local_events: Arc<DashMap<TransactionId, Vec<Arc<Event<TransactionPayload, TransactionId>>>>>,

    transaction_table: Arc<DashMap<TransactionId, TransactionState>>,
    journal_lookup_table: Arc<DashMap<JournalId, Vec<TransactionId>>>,
}

impl TransactionMemoryStore {
    pub fn new() -> Self {
        Self {
            global_events: Arc::new(Mutex::new(Vec::new())),
            local_events: Arc::new(DashMap::new()),
            transaction_table: Arc::new(DashMap::new()),
            journal_lookup_table: Arc::new(DashMap::new()),
        }
    }
}

impl EventStore for TransactionMemoryStore {
    type Id = TransactionId;
    type EventId = u64;
    type Payload = TransactionPayload;
    type Error = TransactionStoreError;

    async fn record(
        &self,
        id: Self::Id,
        authority: Authority,
        payload: Self::Payload,
    ) -> Result<u64, Self::Error> {
        let (event_id, event) = {
            let mut global_events = self.global_events.lock().await;
            let event_id = global_events.len() as u64;
            let event = Arc::new(Event::new(payload.clone(), id, event_id, authority));
            global_events.push(event.clone());
            (event_id, event)
        };

        if let TransactionPayload::CreatedTransaction {
            journal_id,
            updates,
        } = payload
        {
            let state = TransactionState {
                id,
                journal_id,
                updates,
            };

            self.local_events.insert(id, vec![event.clone()]);
            self.transaction_table.insert(id, state);

            self.journal_lookup_table
                .entry(journal_id)
                .or_default()
                .push(id);

            Ok(event_id)
        } else if let Some(mut local_events) = self.local_events.get_mut(&id)
            && let Some(mut state) = self.transaction_table.get_mut(&id)
        {
            state.apply(payload);
            local_events.push(event);
            Ok(event_id)
        } else {
            Err(InvalidTransaction(id))
        }
    }

    async fn get_events(
        &self,
        id: TransactionId,
        after: u64,
        limit: u64,
    ) -> Result<Vec<Event<TransactionPayload, TransactionId>>, Self::Error> {
        let events = self.local_events.get(&id).ok_or(InvalidTransaction(id))?;

        // avoid a panic fn start > len
        if after >= events.len() as u64 {
            return Ok(Vec::new());
        }

        // clamp the end value to the vector length
        let end = std::cmp::min(events.len(), (after + limit + 1) as usize);

        Ok(events[(after + 1) as usize..end]
            .iter()
            .map(|t| t.deref().clone())
            .collect())
    }
}

impl TransactionStore for TransactionMemoryStore {
    async fn get_journal_transactions(
        &self,
        journal_id: JournalId,
    ) -> TransactionStoreResult<Vec<TransactionId>> {
        Ok(self
            .journal_lookup_table
            .get(&journal_id)
            .map(|s| (*s).clone())
            .unwrap_or_default())
    }

    async fn get_transaction(
        &self,
        transaction_id: &TransactionId,
    ) -> TransactionStoreResult<Option<TransactionState>> {
        Ok(self
            .transaction_table
            .get(transaction_id)
            .map(|s| (*s).clone()))
    }

    async fn get_transaction_authority(
        &self,
        transaction_id: &TransactionId,
    ) -> TransactionStoreResult<Authority> {
        Ok(self
            .local_events
            .get(transaction_id)
            .ok_or(InvalidTransaction(*transaction_id))?
            .first()
            .ok_or(InvalidTransaction(*transaction_id))?
            .authority
            .clone())
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
    type Err = TransactionStoreError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Dr" => Ok(Self::Debit),
            "Cr" => Ok(Self::Credit),
            _ => Err(InvalidEntryType(s.to_string())),
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
pub enum TransactionPayload {
    CreatedTransaction {
        journal_id: JournalId,
        updates: Vec<BalanceUpdate>,
    },
    UpdatedBalancedUpdates {
        new_balanceupdates: Vec<BalanceUpdate>,
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

impl TransactionPayload {
    #[expect(dead_code)]
    fn get_type(&self) -> TransactionEventType {
        use TransactionEventType::*;
        match self {
            Self::CreatedTransaction { .. } => Created,
            Self::UpdatedBalancedUpdates { .. } => UpdatedBalanceUpdates,
        }
    }
}

impl Type<sqlx::Postgres> for TransactionPayload {
    fn type_info() -> <sqlx::Postgres as sqlx::Database>::TypeInfo {
        <&[u8] as Type<sqlx::Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, sqlx::Postgres> for TransactionPayload {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Postgres as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let bytes: Vec<u8> = postcard::to_allocvec(self)?;
        <&[u8] as Encode<sqlx::Postgres>>::encode(&bytes, buf)
    }
}

impl<'r> Decode<'r, sqlx::Postgres> for TransactionPayload {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let bytes = <&[u8] as Decode<sqlx::Postgres>>::decode(value)?;
        Ok(postcard::from_bytes::<TransactionPayload>(bytes)?)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TransactionState {
    pub id: TransactionId,
    journal_id: JournalId,
    pub updates: Vec<BalanceUpdate>,
}

impl TransactionState {
    pub fn apply(&mut self, event: TransactionPayload) {
        match event {
            TransactionPayload::CreatedTransaction {
                journal_id,
                updates,
            } => {
                self.journal_id = journal_id;
                self.updates = updates;
            }
            TransactionPayload::UpdatedBalancedUpdates {
                new_balanceupdates, ..
            } => self.updates = new_balanceupdates,
        }
    }
}

#[cfg(test)]
mod test_transaction {
    use super::TransactionPayload;
    use crate::ident::AccountId;
    use crate::ident::JournalId;
    use crate::transaction::BalanceUpdate;
    use crate::transaction::EntryType;
    use sqlx::PgPool;
    use sqlx::prelude::FromRow;

    #[sqlx::test]
    async fn test_encode_decode_transaction_event(pool: PgPool) {
        let original_event = TransactionPayload::CreatedTransaction {
            journal_id: JournalId::new(),
            updates: vec![BalanceUpdate {
                journal_id: JournalId::new(),
                account_id: AccountId::new(),
                amount: 45,
                entry_type: EntryType::Debit,
            }],
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

        let event: TransactionPayload = sqlx::query_scalar(
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
            event: TransactionPayload,
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
