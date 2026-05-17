pub mod commands;
pub mod service;
pub mod views;

pub use service::TransactionService;

use crate::ident::Ident;
use crate::store::universal::{GetPayloadUsage, PayloadUsage, SequenceId};
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

use crate::account::{AccountId, AccountStoreError};
use crate::event::Event;
use crate::event::EventStore;
use crate::journal::Permissions;
use crate::journal::{JournalId, JournalStoreError};
use crate::postcard::Postcard;
use crate::store::universal::registry::{AnyPayload, EntityType};
use crate::transaction::TransactionStoreError::InvalidEntryType;
use crate::{entity, payload, state};
use TransactionStoreError::InvalidTransaction;
use dashmap::DashMap;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde::Serialize;
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

        match payload.usage(id, SequenceId(0)) {
            PayloadUsage::CreatesState(state) => {
                let journal_id = state.journal_id;
                self.local_events.insert(id, vec![event.clone()]);
                self.transaction_table.insert(id, state);

                self.journal_lookup_table
                    .entry(journal_id)
                    .or_default()
                    .push(id);
            }
            PayloadUsage::ModifiesState(mod_fn) => {
                if let Some(mut local_events) = self.local_events.get_mut(&id)
                    && let Some(mut state) = self.transaction_table.get_mut(&id)
                {
                    mod_fn(&mut state);
                    local_events.push(event);
                } else {
                    return Err(InvalidTransaction(id));
                }
            }
        }
        Ok(event_id)
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

entity!(
    TransactionEntity,
    EntityType::Transaction,
    TransactionId,
    TransactionPayload,
    TransactionState,
    Ident::new16()
);

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BalanceUpdate {
    pub journal_id: JournalId,
    pub account_id: AccountId,
    pub amount: u64,
    pub entry_type: EntryType,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TransactionModifiedPayload {
    UpdatedBalancedUpdates {
        new_balanceupdates: Vec<BalanceUpdate>,
    },
    Deleted,
}

payload! {
    AnyPayload::Transaction,

    pub enum TransactionPayload {
        Created {
            journal_id: JournalId,
            updates: Vec<BalanceUpdate>,
        },
        Modified(TransactionModifiedPayload)
    }
}
state! {
    #[diesel(table_name = crate::schema::transactions)]
    pub struct TransactionState {
        pub id: TransactionId,
        pub journal_id: JournalId,
        pub updates: Postcard<Vec<BalanceUpdate>>,
        pub deleted: bool,
        pub as_of: SequenceId
    }
}

impl GetPayloadUsage<TransactionEntity> for TransactionPayload {
    fn usage<T: Into<TransactionId>>(
        self,
        entity_id: T,
        sequence_id: SequenceId,
    ) -> PayloadUsage<TransactionEntity> {
        match self {
            TransactionPayload::Created {
                journal_id,
                updates,
            } => PayloadUsage::CreatesState(TransactionState {
                id: entity_id.into(),
                journal_id,
                updates: Postcard(updates),
                deleted: false,
                as_of: sequence_id,
            }),
            TransactionPayload::Modified(modified_payload) => {
                PayloadUsage::ModifiesState(Box::new(move |state: &mut TransactionState| {
                    match modified_payload {
                        TransactionModifiedPayload::UpdatedBalancedUpdates {
                            new_balanceupdates,
                        } => state.updates = Postcard(new_balanceupdates),
                        TransactionModifiedPayload::Deleted => state.deleted = true,
                    }
                    state.as_of = sequence_id;
                }))
            }
        }
    }
}
