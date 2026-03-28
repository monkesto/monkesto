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
use crate::authority::UserId;
use crate::event::{Event, EventStore};
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::journal::JournalStoreError;
use crate::journal::Permissions;
use crate::name::Name;
use crate::transaction::EntryType;
use crate::transaction::TransactionEvent;
use crate::transaction::TransactionState;
use chrono::DateTime;
use chrono::Utc;
use dashmap::DashMap;
use serde::Deserialize;
use serde::Serialize;
use sqlx::Decode;
use sqlx::Encode;
use sqlx::Type;
use sqlx::postgres::PgValueRef;
use std::collections::HashSet;
use std::ops::Deref;
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum AccountPayload {
    Created {
        journal_id: JournalId,
        name: Name,
        parent_account_id: Option<AccountId>,
    },
    Renamed {
        new_name: Name,
    },
    Deleted
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AccountState {
    pub id: AccountId,
    pub name: Name,
    pub journal_id: JournalId,
    pub balance: i64,
    pub deleted: bool,
    pub parent_account_id: Option<AccountId>,
}

impl AccountState {
    pub fn apply(&mut self, event: AccountPayload) {
        use AccountPayload::*;
        match event {
                Created {
                journal_id,
                name,
                parent_account_id,
            } => {
                self.journal_id = journal_id;
                self.name = name;
                self.parent_account_id = parent_account_id;
            }
            Renamed { new_name, .. } => {
                self.name = new_name;
            }
            Deleted { .. } => {
                self.deleted = true;
            }
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

    async fn get_account(&self, account_id: &AccountId)
    -> AccountStoreResult<Option<AccountState>>;

    async fn update_balances(
        &self,
        transaction_event: &TransactionEvent,
        old_transaction: Option<&TransactionState>,
    ) -> AccountStoreResult<()>;
}

#[derive(Clone)]
pub struct AccountMemoryStore {
    global_events: Arc<Mutex<Vec<Arc<Event<AccountPayload, AccountId>>>>>,
    local_events: Arc<DashMap<AccountId, Vec<Arc<Event<AccountPayload, AccountId>>>>>,
    account_table: Arc<DashMap<AccountId, AccountState>>,

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
            AccountPayload::Created { journal_id, name, parent_account_id, } => {
                self.local_events.insert(id, vec![event.clone()]);

                let state = AccountState {
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
            },
            _ => {
                if let Some(mut local_events) = self.local_events.get_mut(&id)
                    && let Some(mut state) = self.account_table.get_mut(&id)
                {
                    local_events.push(event.clone());
                    state.apply(payload);

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
    ) -> AccountStoreResult<Option<AccountState>> {
        Ok(self.account_table.get(account_id).map(|s| (*s).clone()))
    }

    async fn update_balances(
        &self,
        transaction_event: &TransactionEvent,
        old_transaction: Option<&TransactionState>,
    ) -> AccountStoreResult<()> {
        match transaction_event {
            TransactionEvent::CreatedTransaction { updates, .. } => {
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
            TransactionEvent::UpdatedBalancedUpdates {
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
                    return Err(TransactionWithoutPriorState(transaction_event.id()));
                }
            }
        }

        Ok(())
    }
}

impl Type<sqlx::Postgres> for AccountEvent {
    fn type_info() -> <sqlx::Postgres as sqlx::Database>::TypeInfo {
        <&[u8] as Type<sqlx::Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, sqlx::Postgres> for AccountEvent {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Postgres as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let bytes: Vec<u8> = postcard::to_allocvec(self)?;
        <&[u8] as Encode<sqlx::Postgres>>::encode(&bytes, buf)
    }
}

impl<'r> Decode<'r, sqlx::Postgres> for AccountEvent {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let bytes = <&[u8] as Decode<sqlx::Postgres>>::decode(value)?;
        Ok(postcard::from_bytes::<AccountEvent>(bytes)?)
    }
}

#[cfg(test)]
mod test_account {
    use super::AccountEvent;
    use crate::authority::UserId;
    use crate::name::Name;
    use sqlx::PgPool;
    use sqlx::prelude::FromRow;

    #[sqlx::test]
    async fn test_encode_decode_account_event(pool: PgPool) {
        let original_event = AccountEvent::Renamed {
            new_name: Name::try_new("New Name".to_string()).expect("name creation failed"),
            updater: UserId::new(),
        };

        sqlx::query(
            r#"
            CREATE TABLE test_account_table (
            event BYTEA
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("failed to create mock account table");

        sqlx::query(
            r#"
            INSERT INTO test_account_table (
            event
            )
            VALUES ($1)
            "#,
        )
        .bind(&original_event)
        .execute(&pool)
        .await
        .expect("failed to insert account into mock table");

        let event: AccountEvent = sqlx::query_scalar(
            r#"
            SELECT event FROM test_account_table
            LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed to fetch account from mock table");

        assert_eq!(event, original_event);

        #[derive(FromRow)]
        struct WrapperType {
            event: AccountEvent,
        }

        let event_wrapper: WrapperType = sqlx::query_as(
            r#"
            SELECT event FROM test_account_table
            LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed to fetch account from mock table");

        assert_eq!(event_wrapper.event, original_event)
    }
}
