pub mod commands;
pub mod service;
pub mod views;

pub use service::AccountService;

use axum::Router;
use axum::routing::get;
use axum_login::login_required;

pub fn router() -> Router<crate::StateType> {
    Router::new()
        .route("/journal/{id}/account", get(views::account_list_page))
        .route(
            "/journal/{id}/createaccount",
            axum::routing::post(commands::create_account),
        )
        .route_layer(login_required!(crate::BackendType, login_url = "/signin"))
}

use crate::authority::Authority;
use crate::authority::UserId;
use crate::event::EventStore;
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::known_errors::KnownErrors;
use crate::known_errors::MonkestoResult;
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
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum AccountEvent {
    Created {
        journal_id: JournalId,
        name: String,
        creator: UserId,
        created_at: DateTime<Utc>,
        parent_account_id: Option<AccountId>,
    },
    Renamed {
        new_name: String,
        updater: UserId,
    },
    Deleted {
        updater: UserId,
    },
}

#[derive(Serialize, Deserialize, Clone)]
pub struct AccountState {
    pub id: AccountId,
    pub name: String,
    pub journal_id: JournalId,
    pub author: UserId,
    pub balance: i64,
    pub created_at: DateTime<Utc>,
    pub deleted: bool,
    pub parent_account_id: Option<AccountId>,
}

impl AccountState {
    pub fn apply(&mut self, event: AccountEvent) {
        match event {
            AccountEvent::Created {
                journal_id,
                name,
                creator,
                created_at,
                parent_account_id,
            } => {
                self.journal_id = journal_id;
                self.name = name;
                self.author = creator;
                self.created_at = created_at;
                self.parent_account_id = parent_account_id;
            }
            AccountEvent::Renamed { new_name, .. } => {
                self.name = new_name;
            }
            AccountEvent::Deleted { .. } => {
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
    + EventStore<Id = AccountId, Event = AccountEvent, Error = KnownErrors>
{
    async fn get_journal_accounts(
        &self,
        journal_id: JournalId,
    ) -> MonkestoResult<HashSet<AccountId>>;

    async fn get_account(&self, account_id: &AccountId) -> MonkestoResult<Option<AccountState>>;

    async fn update_balances(
        &self,
        transaction_event: &TransactionEvent,
        old_transaction: Option<&TransactionState>,
    ) -> MonkestoResult<()>;
}

#[derive(Clone)]
pub struct AccountMemoryStore {
    events: Arc<DashMap<AccountId, Vec<AccountEvent>>>,
    account_table: Arc<DashMap<AccountId, AccountState>>,

    account_lookup_table: Arc<DashMap<JournalId, Vec<AccountId>>>,
}

impl AccountMemoryStore {
    pub fn new() -> Self {
        Self {
            events: Arc::new(DashMap::new()),
            account_table: Arc::new(DashMap::new()),
            account_lookup_table: Arc::new(DashMap::new()),
        }
    }
}

impl EventStore for AccountMemoryStore {
    type Id = AccountId;
    type EventId = ();

    type Event = AccountEvent;
    type Error = KnownErrors;

    async fn record(
        &self,
        id: AccountId,
        _by: Authority,
        event: AccountEvent,
    ) -> Result<(), KnownErrors> {
        if let AccountEvent::Created {
            journal_id,
            name,
            creator,
            created_at,
            parent_account_id,
        } = event.clone()
        {
            self.events.insert(id, vec![event]);

            let state = AccountState {
                id,
                name,
                journal_id,
                author: creator,
                balance: 0,
                created_at,
                deleted: false,
                parent_account_id,
            };

            self.account_table.insert(id, state);

            self.account_lookup_table
                .entry(journal_id)
                .or_default()
                .push(id);

            Ok(())
        } else if let Some(mut events) = self.events.get_mut(&id)
            && let Some(mut state) = self.account_table.get_mut(&id)
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
        id: AccountId,
        after: usize,
        limit: usize,
    ) -> Result<Vec<AccountEvent>, KnownErrors> {
        let events = self
            .events
            .get(&id)
            .ok_or(KnownErrors::AccountDoesntExist { id })?;

        // avoid a panic if start > len
        if after >= events.len() {
            return Ok(Vec::new());
        }

        // clamp the end value to the vector length
        let end = std::cmp::min(events.len(), after + limit + 1);

        Ok(events[after + 1..end].to_vec())
    }
}

impl AccountStore for AccountMemoryStore {
    async fn get_journal_accounts(
        &self,
        journal_id: JournalId,
    ) -> MonkestoResult<HashSet<AccountId>> {
        Ok(self
            .account_lookup_table
            .get(&journal_id)
            .map(|s| (*s).clone())
            .unwrap_or_default()
            .iter()
            .copied()
            .collect::<HashSet<AccountId>>())
    }

    async fn get_account(&self, account_id: &AccountId) -> MonkestoResult<Option<AccountState>> {
        Ok(self.account_table.get(account_id).map(|s| (*s).clone()))
    }

    async fn update_balances(
        &self,
        transaction_event: &TransactionEvent,
        old_transaction: Option<&TransactionState>,
    ) -> MonkestoResult<()> {
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
                        return Err(KnownErrors::AccountDoesntExist {
                            id: update.account_id,
                        });
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
                            return Err(KnownErrors::AccountDoesntExist {
                                id: update.account_id,
                            });
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
                            return Err(KnownErrors::AccountDoesntExist {
                                id: update.account_id,
                            });
                        }
                    }
                } else {
                    return Err(KnownErrors::InternalError {
                        context: "account store tried to update a transaction without an old state"
                            .to_string(),
                    });
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
    use sqlx::PgPool;
    use sqlx::prelude::FromRow;

    #[sqlx::test]
    async fn test_encode_decode_account_event(pool: PgPool) {
        let original_event = AccountEvent::Renamed {
            new_name: "New Name".to_string(),
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
