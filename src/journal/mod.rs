pub mod commands;
pub mod layout;
pub mod queries;
pub mod views;

use crate::{cuid::Cuid, known_errors::KnownErrors};
use bitflags::bitflags;
use chrono::{DateTime, Utc};
use postcard::{from_bytes, to_allocvec};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, query_as, query_scalar};
use std::collections::HashMap;

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
    pub account_id: Cuid,
    pub changed_by: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Transaction {
    pub author: Cuid,
    pub updates: Vec<BalanceUpdate>,
}

#[derive(Default, Serialize, Deserialize, Clone, Debug, Copy)]
pub struct JournalTenantInfo {
    pub tenant_permissions: Permissions,
    pub inviting_user: Cuid,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum JournalEvent {
    Created {
        name: String,
        owner: Cuid,
    },
    Renamed {
        name: String,
    },
    AddedTenant {
        id: Cuid,
        tenant_info: JournalTenantInfo,
    },
    CreatedAccount {
        name: String,
        id: Cuid,
        created_by: Cuid,
        created_at: DateTime<Utc>,
    },
    DeletedAccount {
        account_id: Cuid,
    },
    AddedEntry {
        transaction: Transaction,
    },
    Deleted,
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "smallint")]
#[repr(i16)]
pub enum JournalEventType {
    Created = 1,
    Renamed = 2,
    AddedTenant = 3,
    CreatedAccount = 4,
    DeletedAccount = 5,
    AddedEntry = 6,
    Deleted = 7,
}

impl JournalEvent {
    fn get_type(&self) -> JournalEventType {
        use JournalEventType::*;
        match self {
            Self::Created { .. } => Created,
            Self::Renamed { .. } => Renamed,
            Self::AddedTenant { .. } => AddedTenant,
            Self::CreatedAccount { .. } => CreatedAccount,
            Self::DeletedAccount { .. } => DeletedAccount,
            Self::AddedEntry { .. } => AddedEntry,
            Self::Deleted => Deleted,
        }
    }

    pub async fn push_db(&self, id: &Cuid, pool: &PgPool) -> Result<i64, KnownErrors> {
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
        .bind(id.as_bytes())
        .bind(self.get_type())
        .bind(payload)
        .fetch_one(pool)
        .await?;

        Ok(id)
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct JournalState {
    pub id: Cuid,
    pub name: String,
    pub created_at: chrono::DateTime<Utc>,
    pub owner: Cuid,
    pub tenants: HashMap<Cuid, JournalTenantInfo>,
    pub accounts: HashMap<Cuid, Account>,
    pub transactions: Vec<Transaction>,
    pub deleted: bool,
}

impl JournalState {
    pub async fn build(
        id: &Cuid,
        event_types: Vec<JournalEventType>,
        pool: &PgPool,
    ) -> Result<Self, KnownErrors> {
        let journal_events = query_as::<_, (Vec<u8>,)>(
            r#"
                SELECT payload FROM journal_events
                WHERE journal_id = $1 AND event_type = ANY($2)
                ORDER BY created_at ASC
                "#,
        )
        .bind(id.as_bytes())
        .bind(event_types)
        .fetch_all(pool)
        .await?;

        let created_at: Option<chrono::DateTime<Utc>> = query_scalar(
            r#"
                SELECT created_at FROM journal_events
                WHERE journal_id = $1 AND event_type = $2
            "#,
        )
        .bind(id.as_bytes())
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
            .try_for_each(|(payload,)| -> Result<(), KnownErrors> {
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

            JournalEvent::AddedTenant { id, tenant_info } => {
                _ = self.tenants.insert(id, tenant_info);
            }

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
            JournalEvent::AddedEntry { transaction } => {
                for balance_update in &transaction.updates {
                    self.accounts
                        .entry(balance_update.account_id)
                        .and_modify(|account| account.balance += balance_update.changed_by);
                }
                self.transactions.push(transaction);
            }
            JournalEvent::Deleted => self.deleted = true,
        }
    }

    pub fn get_user_permissions(&self, user_id: &Cuid) -> Permissions {
        if self.owner == *user_id {
            Permissions::all()
        } else if let Some(tenant_info) = self.tenants.get(user_id) {
            tenant_info.tenant_permissions
        } else {
            Permissions::empty()
        }
    }
}

#[allow(dead_code)]
pub struct SharedJournal {
    pub id: Cuid,
    pub info: JournalTenantInfo,
}

#[allow(dead_code)]
pub struct SharedAndPendingJournals {
    pub shared: HashMap<Cuid, JournalTenantInfo>,
    pub pending: HashMap<Cuid, JournalTenantInfo>,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Clone)]
pub struct Account {
    pub name: String,
    pub created_by: Cuid,
    pub created_at: chrono::DateTime<Utc>,
    pub balance: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum AssociatedJournal {
    Owned {
        name: String,
        created_at: chrono::DateTime<Utc>,
    },
    Shared {
        name: String,
        created_at: chrono::DateTime<Utc>,
        tenant_info: JournalTenantInfo,
    },
}

impl AssociatedJournal {
    #[allow(dead_code)]
    fn has_permission(&self, permissions: Permissions) -> bool {
        match self {
            Self::Owned { .. } => true,
            Self::Shared { tenant_info, .. } => {
                tenant_info.tenant_permissions.contains(permissions)
            }
        }
    }
}

impl AssociatedJournal {
    pub fn get_name(&self) -> String {
        match self {
            Self::Owned { name, .. } => name.clone(),
            Self::Shared { name, .. } => name.clone(),
        }
    }
    pub fn get_created_at(&self) -> chrono::DateTime<Utc> {
        match self {
            Self::Owned { created_at, .. } => *created_at,
            Self::Shared { created_at, .. } => *created_at,
        }
    }
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Clone)]
pub struct JournalInvite {
    pub id: Cuid,
    pub name: String,
    pub tenant_info: JournalTenantInfo,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Clone)]
pub struct TransactionWithUsername {
    pub author: String,
    pub updates: Vec<BalanceUpdate>,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Clone)]
pub struct TransactionWithTimeStamp {
    pub transaction: TransactionWithUsername,
    pub timestamp: chrono::DateTime<Utc>,
}
