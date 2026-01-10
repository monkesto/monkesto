pub mod commands;
pub mod layout;
pub mod queries;
pub mod transaction;
pub mod views;
use crate::{
    cuid::Cuid,
    known_errors::{KnownErrors, MonkestoResult},
};
use async_trait::async_trait;
use bitflags::bitflags;
use chrono::{DateTime, Utc};
use postcard::{from_bytes, to_allocvec};
use serde::{Deserialize, Serialize};
use sqlx::{Decode, Encode, PgPool, Type, postgres::PgValueRef, query_as, query_scalar};
use std::collections::HashMap;

#[async_trait]
#[allow(dead_code)]
pub trait JournalStore {
    /// creates a new journal state in the event store with the data from the creation event
    ///
    /// it should return an error if the event passed in is not a creation event
    async fn create_journal(&self, creation_event: JournalState) -> MonkestoResult<()>;

    /// adds a UserEvent to the event store and updates the cached state
    async fn push_event(&self, journal_id: &Cuid, event: JournalEvent) -> MonkestoResult<()>;

    /// returns the cached state of the user
    async fn get_journal(&self, journal_id: &Cuid) -> MonkestoResult<JournalState>;
}

#[allow(dead_code)]
pub struct Journals {
    store: dyn JournalStore,
}

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

#[derive(Default, Serialize, Deserialize, Clone, Debug, Copy, PartialEq)]
pub struct JournalTenantInfo {
    pub tenant_permissions: Permissions,
    pub inviting_user: Cuid,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum JournalEvent {
    Created {
        id: Cuid,
        name: String,
        created_at: chrono::DateTime<Utc>,
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
        transaction_id: Cuid,
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

#[derive(Default, Serialize, Deserialize)]
pub struct JournalState {
    pub id: Cuid,
    pub name: String,
    pub created_at: chrono::DateTime<Utc>,
    pub owner: Cuid,
    pub tenants: HashMap<Cuid, JournalTenantInfo>,
    pub accounts: HashMap<Cuid, Account>,
    pub transactions: Vec<Cuid>,
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
            JournalEvent::Created {
                id,
                name,
                owner,
                created_at,
            } => {
                self.id = id;
                self.name = name;
                self.created_at = created_at;
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
            JournalEvent::AddedEntry { transaction_id } => {
                self.transactions.push(transaction_id);
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

#[cfg(test)]
mod test_user {
    use crate::cuid::Cuid;
    use chrono::Utc;
    use sqlx::{PgPool, prelude::FromRow};

    use super::JournalEvent;

    #[sqlx::test]
    async fn test_encode_decode_journalevent(pool: PgPool) {
        let original_event = JournalEvent::CreatedAccount {
            name: "test".into(),
            id: Cuid::new10(),
            created_by: Cuid::new10(),
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
