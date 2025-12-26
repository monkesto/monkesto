use bitflags::bitflags;
use chrono::Utc;
use leptos::prelude::ServerFnError;
use postcard::{from_bytes, to_allocvec};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, query_as, query_scalar};
use std::collections::HashMap;
use uuid::Uuid;

bitflags! {
    #[derive(Serialize, Deserialize, Hash, Default, Debug, Clone, Copy, PartialEq)]
    pub struct Permissions: i16 {
        const READ = 1 << 0;
        const ADDACCOUNT = 1 << 1;
        const APPENDTRANSACTION = 1 << 2;
        const INVITE = 1 << 3;
        const DELETE = 1 << 4;
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BalanceUpdate {
    pub account_name: String,
    pub changed_by: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Transaction {
    pub author: Uuid,
    pub updates: Vec<BalanceUpdate>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum JournalEvent {
    Created { name: String, owner: Uuid },
    Renamed { name: String },
    CreatedAccount { account_name: String },
    DeletedAccount { account_name: String },
    AddedEntry { transaction: Transaction },
    Deleted,
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "smallint")]
#[repr(i16)]
pub enum JournalEventType {
    Created = 1,
    Renamed = 2,
    CreatedAccount = 3,
    DeletedAccount = 4,
    AddedEntry = 5,
    Deleted = 6,
}

impl JournalEvent {
    fn get_type(&self) -> JournalEventType {
        use JournalEventType::*;
        match self {
            Self::Created { .. } => Created,
            Self::Renamed { .. } => Renamed,
            Self::CreatedAccount { .. } => CreatedAccount,
            Self::DeletedAccount { .. } => DeletedAccount,
            Self::AddedEntry { .. } => AddedEntry,
            Self::Deleted => Deleted,
        }
    }

    pub async fn push_db(&self, uuid: &Uuid, pool: &PgPool) -> Result<i64, ServerFnError> {
        let payload: Vec<u8> = to_allocvec(self)?;

        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO journal_events (
                journal_id,
                event_type,
                payload
            )
            VALUES ($1, $2, $3)
            RETURNING id
            "#,
        )
        .bind(uuid)
        .bind(self.get_type())
        .bind(payload)
        .fetch_one(pool)
        .await?;

        Ok(id)
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct JournalState {
    pub id: Uuid,
    pub name: String,
    pub created_at: chrono::DateTime<Utc>,
    pub owner: Uuid,
    pub accounts: HashMap<String, i64>,
    pub transactions: Vec<Transaction>,
    pub deleted: bool,
}

impl JournalState {
    pub async fn build(
        id: &Uuid,
        event_types: Vec<JournalEventType>,
        pool: &PgPool,
    ) -> Result<Self, ServerFnError> {
        let journal_events = query_as::<_, (Vec<u8>,)>(
            r#"
                SELECT payload FROM journal_events
                WHERE journal_id = $1 AND event_type = ANY($2)
                ORDER BY created_at ASC
                "#,
        )
        .bind(id)
        .bind(event_types)
        .fetch_all(pool)
        .await?;

        let created_at: Option<chrono::DateTime<Utc>> = query_scalar(
            r#"
                SELECT created_at FROM journal_events
                WHERE journal_id = $1 AND event_type = $2
            "#,
        )
        .bind(id)
        .bind(JournalEventType::Created)
        .fetch_optional(pool)
        .await?;

        let mut aggregate = Self {
            id: *id,
            created_at: created_at.unwrap_or_default(),
            ..Default::default()
        };

        journal_events
            .into_iter()
            .try_for_each(|(payload,)| -> Result<(), ServerFnError> {
                aggregate.apply(from_bytes::<JournalEvent>(&payload)?);
                Ok(())
            })?;

        Ok(aggregate)
    }

    pub fn apply(&mut self, event: JournalEvent) {
        match event {
            JournalEvent::Created { name, owner } => {
                self.name = name;
                self.owner = owner;
            }

            JournalEvent::Renamed { name } => self.name = name,

            JournalEvent::CreatedAccount { account_name } => {
                _ = self.accounts.insert(account_name, 0)
            }
            JournalEvent::DeletedAccount { account_name } => {
                _ = self.accounts.remove(&account_name)
            }
            JournalEvent::AddedEntry { transaction } => {
                for balance_update in &transaction.updates {
                    self.accounts
                        .entry(balance_update.account_name.clone())
                        .and_modify(|balance| *balance += balance_update.changed_by);
                }
                self.transactions.push(transaction);
            }
            JournalEvent::Deleted => self.deleted = true,
        }
    }
}

#[derive(Default, Serialize, Deserialize, Clone, Debug)]
pub struct JournalTenantInfo {
    pub tenant_permissions: Permissions,
    pub inviting_user: Uuid,
    pub journal_owner: Uuid,
}

pub struct SharedJournal {
    pub id: Uuid,
    pub info: JournalTenantInfo,
}

pub struct SharedAndPendingJournals {
    pub shared: HashMap<Uuid, JournalTenantInfo>,
    pub pending: HashMap<Uuid, JournalTenantInfo>,
}

pub async fn get_name_from_id(id: &Uuid, pool: &PgPool) -> Result<Option<String>, ServerFnError> {
    let journal_state = JournalState::build(
        id,
        vec![JournalEventType::Created, JournalEventType::Renamed],
        pool,
    )
    .await?;
    if journal_state.name.is_empty() {
        return Ok(None);
    }
    Ok(Some(journal_state.name))
}
