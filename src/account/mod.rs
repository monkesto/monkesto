pub mod commands;
pub mod service;
pub mod views;

pub use service::AccountService;

use axum::Router;
use axum::routing::get;
use axum_login::login_required;

#[derive(Error, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum AccountStoreError {
    #[error("An account with the id {0} doesn't exist")]
    InvalidAccount(AccountId),

    #[error("A journal with the id {0} doesn't exist")]
    InvalidJournal(JournalId),

    #[error(
        "Tried to update the account states for the transaction {0}, but the prior state wasn't provided"
    )]
    TransactionWithoutPriorState(TransactionId),

    #[error("An account with the id {0} already exists")]
    AccountExists(AccountId),

    #[error("The user doesn't have the {:?} permission", .0)]
    PermissionError(Permissions),

    #[error("The journal store returned an error: {0}")]
    JournalError(#[from] JournalStoreError),
}

pub type AccountStoreResult<T> = Result<T, AccountStoreError>;

pub fn router() -> Router<crate::StateType> {
    Router::new()
        .route("/journal/{id}/account", get(views::account_list_page))
        .route(
            "/journal/{id}/createaccount",
            axum::routing::post(commands::create_account),
        )
        .route_layer(login_required!(crate::BackendType, login_url = "/signin"))
}

use crate::account::AccountStoreError::InvalidAccount;
use crate::account::AccountStoreError::TransactionWithoutPriorState;
use crate::authority::Authority;
use crate::event::Event;
use crate::event::EventStore;
use crate::ident::AccountEntity;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::ident::{AccountId, ProjectionFromPayloadError};
use crate::journal::JournalStoreError;
use crate::journal::Permissions;
use crate::name::Name;
use crate::store::universal::{ApplyPayload, PayloadWithId};
use crate::transaction::EntryType;
use crate::transaction::TransactionPayload;
use crate::transaction::TransactionProjection;
use dashmap::DashMap;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;
use std::ops::Deref;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Payload)]
pub enum AccountPayload {
    Created {
        journal_id: JournalId,
        name: Name,
        parent_account_id: Option<AccountId>,
    },
    Renamed {
        new_name: Name,
    },
    Deleted,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AccountProjection {
    pub id: AccountId,
    pub name: Name,
    pub journal_id: JournalId,
    pub balance: i64,
    pub deleted: bool,
    pub parent_account_id: Option<AccountId>,
}

impl ApplyPayload<AccountEntity> for AccountProjection {
    fn apply(&mut self, payload: &AccountPayload) -> &mut Self {
        match payload {
            AccountPayload::Created { .. } => {}
            AccountPayload::Renamed { new_name } => self.name = new_name.clone(),
            AccountPayload::Deleted => self.deleted = true,
        }
        self
    }
}
impl TryFrom<PayloadWithId<AccountEntity>> for AccountProjection {
    type Error = ProjectionFromPayloadError;
    fn try_from(value: PayloadWithId<AccountEntity>) -> Result<Self, ProjectionFromPayloadError> {
        match value.payload {
            AccountPayload::Created {
                journal_id,
                name,
                parent_account_id,
            } => Ok(Self {
                id: value.id,
                name,
                journal_id,
                balance: 0,
                deleted: false,
                parent_account_id,
            }),
            _ => Err(ProjectionFromPayloadError::IncorrectVariant(format!(
                "{:?}",
                value.payload
            ))),
        }
    }
}

pub trait AccountStore:
    Clone
    + Send
    + Sync
    + 'static
    + EventStore<Id = AccountId, Payload = AccountPayload, Error = AccountStoreError>
{
    async fn get_journal_accounts(
        &self,
        journal_id: JournalId,
    ) -> AccountStoreResult<HashSet<AccountId>>;

    async fn get_account(
        &self,
        account_id: &AccountId,
    ) -> AccountStoreResult<Option<AccountProjection>>;

    async fn update_balances(
        &self,
        transaction_id: TransactionId,
        transaction_event: &TransactionPayload,
        old_transaction: Option<&TransactionProjection>,
    ) -> AccountStoreResult<()>;
}

#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub struct AccountMemoryStore {
    global_events: Arc<Mutex<Vec<Arc<Event<AccountPayload, AccountId>>>>>,
    local_events: Arc<DashMap<AccountId, Vec<Arc<Event<AccountPayload, AccountId>>>>>,
    account_table: Arc<DashMap<AccountId, AccountProjection>>,

    account_lookup_table: Arc<DashMap<JournalId, Vec<AccountId>>>,
}

impl AccountMemoryStore {
    pub fn new() -> Self {
        Self {
            global_events: Arc::new(Mutex::new(Vec::new())),
            local_events: Arc::new(DashMap::new()),
            account_table: Arc::new(DashMap::new()),
            account_lookup_table: Arc::new(DashMap::new()),
        }
    }
}

impl EventStore for AccountMemoryStore {
    type Id = AccountId;
    type EventId = u64;
    type Payload = AccountPayload;
    type Error = AccountStoreError;

    async fn record(
        &self,
        id: AccountId,
        authority: Authority,
        payload: AccountPayload,
    ) -> AccountStoreResult<u64> {
        let (event_id, event) = {
            let mut global_events = self.global_events.lock().await;
            let event_id = global_events.len() as u64;
            let event = Arc::new(Event::new(payload.clone(), id, event_id, authority));
            global_events.push(event.clone());
            (event_id, event)
        };

        match payload.clone() {
            AccountPayload::Created {
                journal_id,
                name,
                parent_account_id,
            } => {
                self.local_events.insert(id, vec![event.clone()]);

                let state = AccountProjection {
                    id,
                    name,
                    journal_id,
                    balance: 0,
                    deleted: false,
                    parent_account_id,
                };

                self.account_table.insert(id, state);

                self.account_lookup_table
                    .entry(journal_id)
                    .or_default()
                    .push(id);

                Ok(event_id)
            }
            _ => {
                if let Some(mut local_events) = self.local_events.get_mut(&id)
                    && let Some(mut state) = self.account_table.get_mut(&id)
                {
                    local_events.push(event.clone());
                    state.apply(&payload);

                    Ok(event_id)
                } else {
                    Err(InvalidAccount(id))
                }
            }
        }
    }

    async fn get_events(
        &self,
        id: AccountId,
        after: u64,
        limit: u64,
    ) -> AccountStoreResult<Vec<Event<AccountPayload, AccountId>>> {
        let events = self.local_events.get(&id).ok_or(InvalidAccount(id))?;

        // avoid a panic if start > len
        if after >= events.len() as u64 {
            return Ok(Vec::new());
        }

        // clamp the end value to the vector length
        let end = std::cmp::min(events.len(), (after + limit + 1) as usize);

        Ok(events[after as usize + 1..end]
            .iter()
            .map(|ev| ev.deref().clone())
            .collect())
    }
}

impl AccountStore for AccountMemoryStore {
    async fn get_journal_accounts(
        &self,
        journal_id: JournalId,
    ) -> AccountStoreResult<HashSet<AccountId>> {
        Ok(self
            .account_lookup_table
            .get(&journal_id)
            .map(|s| (*s).clone())
            .unwrap_or_default()
            .iter()
            .copied()
            .collect::<HashSet<AccountId>>())
    }

    async fn get_account(
        &self,
        account_id: &AccountId,
    ) -> AccountStoreResult<Option<AccountProjection>> {
        Ok(self.account_table.get(account_id).map(|s| (*s).clone()))
    }

    async fn update_balances(
        &self,
        transaction_id: TransactionId,
        transaction_event: &TransactionPayload,
        old_transaction: Option<&TransactionProjection>,
    ) -> AccountStoreResult<()> {
        match transaction_event {
            TransactionPayload::Created { updates, .. } => {
                for update in updates {
                    if let Some(mut account_state) = self.account_table.get_mut(&update.account_id)
                    {
                        match update.entry_type {
                            EntryType::Debit => {
                                account_state.balance -= update.amount as i64;
                            }
                            EntryType::Credit => {
                                account_state.balance += update.amount as i64;
                            }
                        }
                    } else {
                        return Err(InvalidAccount(update.account_id));
                    }
                }
            }
            TransactionPayload::UpdatedBalancedUpdates {
                new_balanceupdates, ..
            } => {
                if let Some(transaction) = old_transaction {
                    // reverse the old transaction
                    for update in transaction.updates.iter() {
                        if let Some(mut account_state) =
                            self.account_table.get_mut(&update.account_id)
                        {
                            match update.entry_type {
                                EntryType::Debit => {
                                    account_state.balance += update.amount as i64;
                                }
                                EntryType::Credit => {
                                    account_state.balance -= update.amount as i64;
                                }
                            }
                        } else {
                            return Err(InvalidAccount(update.account_id));
                        }
                    }

                    for update in new_balanceupdates {
                        if let Some(mut account_state) =
                            self.account_table.get_mut(&update.account_id)
                        {
                            match update.entry_type {
                                EntryType::Debit => {
                                    account_state.balance -= update.amount as i64;
                                }
                                EntryType::Credit => {
                                    account_state.balance += update.amount as i64;
                                }
                            }
                        } else {
                            Err(InvalidAccount(update.account_id))?
                        }
                    }
                } else {
                    return Err(TransactionWithoutPriorState(transaction_id));
                }
            }
            TransactionPayload::Deleted => {
                if let Some(transaction) = old_transaction {
                    for update in transaction.updates.iter() {
                        if let Some(mut account_state) =
                            self.account_table.get_mut(&update.account_id)
                        {
                            match update.entry_type {
                                EntryType::Debit => {
                                    account_state.balance += update.amount as i64;
                                }
                                EntryType::Credit => {
                                    account_state.balance -= update.amount as i64;
                                }
                            }
                        } else {
                            return Err(InvalidAccount(update.account_id));
                        }
                    }
                } else {
                    return Err(TransactionWithoutPriorState(transaction_id));
                }
            }
        }

        Ok(())
    }
}
