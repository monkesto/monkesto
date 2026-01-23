pub mod commands;
pub mod layout;
pub mod transaction;
pub mod views;

use crate::{
    ident::{AccountId, JournalId, TransactionId, UserId},
    journal::transaction::{
        EntryType, TransactionEvent, TransactionState, TransactionStore, TransasctionMemoryStore,
    },
    known_errors::{KnownErrors, MonkestoResult},
};
use async_trait::async_trait;
use bitflags::bitflags;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::{Decode, Encode, Type, postgres::PgValueRef};
use std::{collections::HashMap, sync::Arc};

#[async_trait]
#[expect(dead_code)]
pub trait JournalStore: Clone + Send + Sync + 'static {
    /// creates a new journal state in the event store with the data from the creation event
    ///
    /// it should return an error if the event passed in is not a creation event
    async fn create_journal(&self, creation_event: JournalEvent) -> MonkestoResult<()>;

    /// adds a UserEvent to the event store and updates the cached state
    async fn push_journal_event(
        &self,
        journal_id: &JournalId,
        event: JournalEvent,
    ) -> MonkestoResult<()>;

    /// intercept transaction events to update account state
    async fn create_transaction(
        &self,
        journal_id: &JournalId,
        creation_event: TransactionEvent,
    ) -> MonkestoResult<()>;

    /// intercept transaction events to update account state
    async fn push_transaction_event(
        &self,
        journal_id: &JournalId,
        transaction_id: &TransactionId,
        event: TransactionEvent,
    ) -> MonkestoResult<()>;

    /// helper function to remove the requirement for directly storing the transaction store in the state
    async fn get_transaction_state(
        &self,
        transaction_id: &TransactionId,
    ) -> MonkestoResult<TransactionState>;

    /// returns the cached state of the user
    async fn get_journal(&self, journal_id: &JournalId) -> MonkestoResult<JournalState>;

    /// returns all journals that a user is a member of (owner or tenant)
    async fn get_user_journals(&self, user_id: &UserId) -> MonkestoResult<Vec<JournalId>>;

    async fn get_permissions(
        &self,
        journal_id: &JournalId,
        user_id: &UserId,
    ) -> MonkestoResult<Permissions> {
        let state = self.get_journal(journal_id).await?;
        if state.owner == *user_id {
            return Ok(Permissions::all());
        }
        Ok(state
            .tenants
            .get(user_id)
            .map(|t| t.tenant_permissions)
            .unwrap_or(Permissions::empty()))
    }

    async fn get_name(&self, journal_id: &JournalId) -> MonkestoResult<String> {
        Ok(self.get_journal(journal_id).await?.name)
    }

    async fn get_creator(&self, journal_id: &JournalId) -> MonkestoResult<UserId> {
        Ok(self.get_journal(journal_id).await?.creator)
    }

    async fn get_created_at(&self, journal_id: &JournalId) -> MonkestoResult<DateTime<Utc>> {
        Ok(self.get_journal(journal_id).await?.created_at)
    }

    async fn get_accounts(
        &self,
        journal_id: &JournalId,
    ) -> MonkestoResult<HashMap<AccountId, Account>> {
        Ok(self.get_journal(journal_id).await?.accounts)
    }

    async fn get_transactions(&self, journal_id: &JournalId) -> MonkestoResult<Vec<TransactionId>> {
        Ok(self.get_journal(journal_id).await?.transactions)
    }

    async fn get_deleted(&self, journal_id: &JournalId) -> MonkestoResult<bool> {
        Ok(self.get_journal(journal_id).await?.deleted)
    }

    async fn seed_journal(
        &self,
        creation_event: JournalEvent,
        update_events: Vec<JournalEvent>,
    ) -> MonkestoResult<()> {
        if let JournalEvent::Created { id, .. } = creation_event {
            self.create_journal(creation_event).await?;

            for event in update_events {
                self.push_journal_event(&id, event).await?;
            }
        } else {
            return Err(KnownErrors::IncorrectEventType);
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct JournalMemoryStore {
    events: Arc<DashMap<JournalId, Vec<JournalEvent>>>,
    journal_table: Arc<DashMap<JournalId, JournalState>>,
    /// Index of user_id -> set of journal_ids they belong to
    user_journals: Arc<DashMap<UserId, std::collections::HashSet<JournalId>>>,

    transaction_store: Arc<TransasctionMemoryStore>,
}

impl JournalMemoryStore {
    pub fn new(transaction_store: Arc<TransasctionMemoryStore>) -> Self {
        Self {
            events: Arc::new(DashMap::new()),
            journal_table: Arc::new(DashMap::new()),
            user_journals: Arc::new(DashMap::new()),
            transaction_store,
        }
    }
}

#[async_trait]
impl JournalStore for JournalMemoryStore {
    async fn create_journal(&self, creation_event: JournalEvent) -> MonkestoResult<()> {
        if let JournalEvent::Created {
            id,
            name,
            created_at,
            creator,
        } = creation_event.clone()
        {
            self.events.insert(id, vec![creation_event]);

            let state = JournalState {
                id,
                name,
                created_at,
                creator,
                owner: creator,
                tenants: HashMap::new(),
                accounts: HashMap::new(),
                transactions: Vec::new(),
                deleted: false,
            };
            self.journal_table.insert(id, state);

            // Add creator to user_journals index
            self.user_journals.entry(creator).or_default().insert(id);

            Ok(())
        } else {
            Err(KnownErrors::InvalidInput)
        }
    }

    async fn push_journal_event(
        &self,
        journal_id: &JournalId,
        event: JournalEvent,
    ) -> MonkestoResult<()> {
        if let Some(mut events) = self.events.get_mut(journal_id)
            && let Some(mut state) = self.journal_table.get_mut(journal_id)
        {
            // Update user_journals index for membership changes
            if let JournalEvent::AddedTenant { id: user_id, .. } = &event {
                self.user_journals
                    .entry(*user_id)
                    .or_default()
                    .insert(*journal_id);
            }

            state.apply(event.clone());
            events.push(event);

            Ok(())
        } else {
            Err(KnownErrors::InvalidJournal)
        }
    }

    async fn create_transaction(
        &self,
        journal_id: &JournalId,
        creation_event: TransactionEvent,
    ) -> MonkestoResult<()> {
        if let TransactionEvent::Created { ref updates, .. } = creation_event {
            if let Some(mut journal) = self.journal_table.get_mut(journal_id) {
                for update in updates {
                    if let Some(account) = journal.accounts.get_mut(&update.account_id) {
                        if update.entry_type == EntryType::Credit {
                            account.balance += update.amount as i64
                        } else {
                            account.balance -= update.amount as i64
                        }
                    } else {
                        return Err(KnownErrors::AccountDoesntExist {
                            id: update.account_id,
                        });
                    }
                }
            } else {
                return Err(KnownErrors::InvalidJournal);
            }

            self.transaction_store
                .create_transaction(creation_event)
                .await
        } else {
            Err(KnownErrors::InvalidInput)
        }
    }

    // TODO: make this update the account stored in the journal
    #[expect(unused_variables)]
    async fn push_transaction_event(
        &self,
        journal_id: &JournalId,
        transaction_id: &TransactionId,
        event: TransactionEvent,
    ) -> MonkestoResult<()> {
        self.transaction_store
            .push_event(transaction_id, event)
            .await
    }

    async fn get_transaction_state(
        &self,
        transaction_id: &TransactionId,
    ) -> MonkestoResult<TransactionState> {
        self.transaction_store.get_transaction(transaction_id).await
    }

    async fn get_journal(&self, journal_id: &JournalId) -> MonkestoResult<JournalState> {
        self.journal_table
            .get(journal_id)
            .map(|state| (*state).clone())
            .ok_or(KnownErrors::InvalidJournal)
    }

    async fn get_user_journals(&self, user_id: &UserId) -> MonkestoResult<Vec<JournalId>> {
        Ok(self
            .user_journals
            .get(user_id)
            .map(|set| set.iter().copied().collect())
            .unwrap_or_default())
    }
}

bitflags! {
    #[derive(Serialize, Deserialize, Hash, Default, Debug, Clone, Copy, PartialEq)]
    pub struct Permissions: i16 {
        const READ = 1 << 0;
        const ADDACCOUNT = 1 << 1;
        const APPENDTRANSACTION = 1 << 2;
        const INVITE = 1 << 3;
        const DELETE = 1 << 4;
        const OWNER = 1 << 5;
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Copy, PartialEq)]
pub struct JournalTenantInfo {
    pub tenant_permissions: Permissions,
    pub inviting_user: UserId,
    pub invited_at: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum JournalEvent {
    Created {
        id: JournalId,
        name: String,
        created_at: chrono::DateTime<Utc>,
        creator: UserId,
    },
    Renamed {
        name: String,
    },
    AddedTenant {
        id: UserId,
        tenant_info: JournalTenantInfo,
    },
    TransferredOwnership {
        new_owner: UserId,
    },
    CreatedAccount {
        name: String,
        id: AccountId,
        created_by: UserId,
        created_at: DateTime<Utc>,
    },
    DeletedAccount {
        account_id: AccountId,
    },
    AddedEntry {
        transaction_id: TransactionId,
    },
    Deleted,
}

impl Type<sqlx::Postgres> for JournalEvent {
    fn type_info() -> <sqlx::Postgres as sqlx::Database>::TypeInfo {
        <&[u8] as Type<sqlx::Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, sqlx::Postgres> for JournalEvent {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Postgres as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let bytes: Vec<u8> = postcard::to_allocvec(self)?;
        <&[u8] as Encode<sqlx::Postgres>>::encode(&bytes, buf)
    }
}

impl<'r> Decode<'r, sqlx::Postgres> for JournalEvent {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let bytes = <&[u8] as Decode<sqlx::Postgres>>::decode(value)?;
        Ok(postcard::from_bytes::<JournalEvent>(bytes)?)
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct JournalState {
    pub id: JournalId,
    pub name: String,
    pub creator: UserId,
    pub created_at: chrono::DateTime<Utc>,
    pub owner: UserId,
    pub tenants: HashMap<UserId, JournalTenantInfo>,
    pub accounts: HashMap<AccountId, Account>,
    pub transactions: Vec<TransactionId>,
    pub deleted: bool,
}

impl JournalState {
    pub fn apply(&mut self, event: JournalEvent) {
        match event {
            JournalEvent::Created {
                id,
                name,
                creator,
                created_at,
            } => {
                self.id = id;
                self.name = name;
                self.created_at = created_at;
                self.creator = creator;
                self.owner = creator;
            }

            JournalEvent::Renamed { name } => self.name = name,

            JournalEvent::AddedTenant { id, tenant_info } => {
                _ = self.tenants.insert(id, tenant_info);
            }

            JournalEvent::TransferredOwnership { new_owner } => self.owner = new_owner,

            JournalEvent::CreatedAccount {
                name,
                id,
                created_at,
                created_by,
            } => {
                _ = self.accounts.insert(
                    id,
                    Account {
                        name,
                        created_by,
                        created_at,
                        balance: 0,
                    },
                )
            }
            JournalEvent::DeletedAccount { account_id } => {
                _ = self.accounts.remove(&account_id);
            }
            JournalEvent::AddedEntry { transaction_id } => {
                self.transactions.push(transaction_id);
            }
            JournalEvent::Deleted => self.deleted = true,
        }
    }

    pub fn get_user_permissions(&self, user_id: &UserId) -> Permissions {
        if self.owner == *user_id {
            Permissions::all()
        } else if let Some(tenant_info) = self.tenants.get(user_id) {
            tenant_info.tenant_permissions
        } else {
            Permissions::empty()
        }
    }
}

#[expect(dead_code)]
pub struct SharedJournal {
    pub id: JournalId,
    pub info: JournalTenantInfo,
}

#[expect(dead_code)]
pub struct SharedAndPendingJournals {
    pub shared: HashMap<JournalId, JournalTenantInfo>,
    pub pending: HashMap<JournalId, JournalTenantInfo>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Account {
    pub name: String,
    pub created_by: UserId,
    pub created_at: chrono::DateTime<Utc>,
    pub balance: i64,
}

#[expect(dead_code)]
#[derive(Serialize, Deserialize, Clone)]
pub struct JournalInvite {
    pub id: UserId,
    pub name: String,
    pub tenant_info: JournalTenantInfo,
}

#[cfg(test)]
mod test_user {
    use crate::ident::{AccountId, UserId};
    use chrono::Utc;
    use sqlx::{PgPool, prelude::FromRow};

    use super::JournalEvent;

    #[sqlx::test]
    async fn test_encode_decode_journalevent(pool: PgPool) {
        let original_event = JournalEvent::CreatedAccount {
            name: "test".into(),
            id: AccountId::new(),
            created_by: UserId::new(),
            created_at: Utc::now(),
        };

        sqlx::query(
            r#"
            CREATE TABLE test_journal_table (
            event BYTEA
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("failed to create mock journal table");

        sqlx::query(
            r#"
            INSERT INTO test_journal_table(
            event
            )
            VALUES ($1)
            "#,
        )
        .bind(&original_event)
        .execute(&pool)
        .await
        .expect("failed to insert journal into mock table");

        let event: JournalEvent = sqlx::query_scalar(
            r#"
            SELECT event FROM test_journal_table
            LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed to fetch journal from mock table");

        assert_eq!(event, original_event);

        #[derive(FromRow)]
        struct WrapperType {
            event: JournalEvent,
        }

        let event_wrapper: WrapperType = sqlx::query_as(
            r#"
            SELECT event FROM test_journal_table
            LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed to fetch journal from mock table");

        assert_eq!(event_wrapper.event, original_event)
    }
}
