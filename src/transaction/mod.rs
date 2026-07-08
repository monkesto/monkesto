pub mod commands;
pub mod service;
pub mod views;

pub use service::TransactionService;
use std::collections::HashMap;

use crate::id::Ident;
use axum::Router;
use axum::routing::get;
use axum_login::login_required;

id!(TransactionId, Ident::new16());

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
use std::ops::{Add, Deref};
use std::str::FromStr;
use std::sync::Arc;

use crate::authority::Authority;

use crate::account::{AccountId, AccountStoreError};
use crate::event::Event;
use crate::event::EventStore;
use crate::id;
use crate::journal::Permissions;
use crate::journal::{JournalId, JournalStoreError};
use crate::transaction::TransactionStoreError::InvalidEntryType;
use TransactionStoreError::InvalidTransaction;
use dashmap::DashMap;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Debug, Error, Serialize, Deserialize, PartialEq)]
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

        match payload.usage(id) {
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

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq)]
pub struct BalanceUpdate {
    pub journal_id: JournalId,
    pub account_id: AccountId,
    pub amount: u64,
    pub entry_type: EntryType,
}

impl Add for BalanceUpdate {
    type Output = BalanceUpdate;

    fn add(self, rhs: Self) -> Self::Output {
        assert_eq!(self.account_id, rhs.account_id);
        assert_eq!(self.journal_id, rhs.journal_id);

        let lhs = match self.entry_type {
            EntryType::Credit => self.amount as i64,
            EntryType::Debit => -(self.amount as i64),
        };

        let rhs = match rhs.entry_type {
            EntryType::Credit => rhs.amount as i64,
            EntryType::Debit => -(rhs.amount as i64),
        };

        let added = lhs + rhs;

        let entry_type = if added < 0 {
            EntryType::Debit
        } else {
            EntryType::Credit
        };

        Self {
            journal_id: self.journal_id,
            account_id: self.account_id,
            amount: added.unsigned_abs(),
            entry_type,
        }
    }
}

impl Add<i64> for BalanceUpdate {
    type Output = i64;

    fn add(self, rhs: i64) -> Self::Output {
        match self.entry_type {
            EntryType::Credit => rhs + self.amount as i64,
            EntryType::Debit => rhs - self.amount as i64,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TransactionModifiedPayload {
    UpdatedBalancedUpdates { difference: Vec<BalanceUpdate> },
    Deleted { difference: Vec<BalanceUpdate> },
}

#[derive(Clone)]
pub enum TransactionPayload {
    Created {
        journal_id: JournalId,
        updates: Vec<BalanceUpdate>,
    },
    #[expect(unused)]
    Modified(TransactionModifiedPayload),
}

#[derive(Clone)]
pub struct TransactionState {
    #[expect(unused)]
    pub id: TransactionId,
    pub journal_id: JournalId,
    pub updates: Vec<BalanceUpdate>,
}

pub enum PayloadUsage {
    CreatesState(TransactionState),
    ModifiesState(Box<dyn FnOnce(&mut TransactionState)>),
}

impl TransactionPayload {
    fn usage<T: Into<TransactionId>>(self, entity_id: T) -> PayloadUsage {
        match self {
            TransactionPayload::Created {
                journal_id,
                updates,
            } => PayloadUsage::CreatesState(TransactionState {
                id: entity_id.into(),
                journal_id,
                updates,
            }),
            TransactionPayload::Modified(modified_payload) => {
                PayloadUsage::ModifiesState(Box::new(move |state: &mut TransactionState| {
                    let mut final_updates = Vec::new();

                    let old_updates = state
                        .updates
                        .iter()
                        .map(|update| {
                            (
                                (update.journal_id, update.account_id),
                                (update.amount, update.entry_type),
                            )
                        })
                        .collect::<HashMap<(JournalId, AccountId), (u64, EntryType)>>();

                    for new_update in match modified_payload {
                        TransactionModifiedPayload::UpdatedBalancedUpdates { difference }
                        | TransactionModifiedPayload::Deleted { difference } => difference,
                    } {
                        if let Some((old_amount, old_entrytype)) =
                            old_updates.get(&(new_update.journal_id, new_update.account_id))
                        {
                            final_updates.push(
                                new_update
                                    + BalanceUpdate {
                                        journal_id: new_update.journal_id,
                                        account_id: new_update.account_id,
                                        amount: *old_amount,
                                        entry_type: *old_entrytype,
                                    },
                            )
                        } else {
                            final_updates.push(new_update)
                        }
                    }

                    state.updates = final_updates;
                }))
            }
        }
    }
}
