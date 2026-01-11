use std::sync::Arc;

use crate::{
    cuid::Cuid,
    known_errors::{KnownErrors, MonkestoResult},
};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use sqlx::{Decode, Encode, Type, postgres::PgValueRef};

#[async_trait]
#[allow(dead_code)]
pub trait TransactionStore: Clone + Send + Sync + 'static {
    /// creates a new transaction state in the event store with the data from the creation event
    ///
    /// it should return an error if the event passed in is not a creation event
    async fn create_transaction(&self, creation_event: TransactionEvent) -> MonkestoResult<()>;

    /// applies a TransactionEvent to the event store and updates the cached state
    async fn push_event(&self, transction_id: &Cuid, event: TransactionEvent)
    -> MonkestoResult<()>;

    async fn get_transaction(&self, transaction_id: &Cuid) -> MonkestoResult<TransactionState>;
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct TransasctionMemoryStore {
    events: Arc<DashMap<Cuid, Vec<TransactionEvent>>>,
    transaction_table: Arc<DashMap<Cuid, TransactionState>>,
}

#[allow(dead_code)]
impl TransasctionMemoryStore {
    pub fn new() -> Self {
        Self {
            events: Arc::new(DashMap::new()),
            transaction_table: Arc::new(DashMap::new()),
        }
    }
}

#[async_trait]
impl TransactionStore for TransasctionMemoryStore {
    async fn create_transaction(&self, creation_event: TransactionEvent) -> MonkestoResult<()> {
        if let TransactionEvent::Created {
            id,
            author,
            description,
            updates,
            created_at,
        } = creation_event.clone()
        {
            self.transaction_table.insert(
                id,
                TransactionState {
                    id,
                    author,
                    description,
                    updates,
                    created_at,
                },
            );

            self.events.insert(id, vec![creation_event]);

            Ok(())
        } else {
            Err(KnownErrors::InvalidInput)
        }
    }

    async fn push_event(
        &self,
        transction_id: &Cuid,
        event: TransactionEvent,
    ) -> MonkestoResult<()> {
        if let Some(mut events) = self.events.get_mut(transction_id)
            && let Some(mut state) = self.transaction_table.get_mut(transction_id)
        {
            events.push(event.clone());
            state.apply(event);
        }

        Ok(())
    }

    async fn get_transaction(&self, transaction_id: &Cuid) -> MonkestoResult<TransactionState> {
        self.transaction_table
            .get(transaction_id)
            .map(|s| (*s).clone())
            .ok_or(KnownErrors::TransactionDoesntExist)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BalanceUpdate {
    pub account_id: Cuid,
    pub changed_by: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub enum TransactionEvent {
    Created {
        id: Cuid,
        author: Cuid,
        description: String,
        updates: Vec<BalanceUpdate>,
        created_at: DateTime<Utc>,
    },
    UpdatedDescription {
        new_desc: String,
        updater: Cuid,
    },
    UpdatedBalancedUpdates {
        new_balanceupdates: Vec<BalanceUpdate>,
        updater: Cuid,
    },
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "smallint")]
#[repr(i16)]
#[allow(dead_code)]
pub enum TransactionEventType {
    Created = 1,
    UpdatedDescription = 2,
    UpdatedBalanceUpdates = 3,
}

impl TransactionEvent {
    #[allow(dead_code)]
    fn get_type(&self) -> TransactionEventType {
        use TransactionEventType::*;
        match self {
            Self::Created { .. } => Created,
            Self::UpdatedBalancedUpdates { .. } => UpdatedBalanceUpdates,
            Self::UpdatedDescription { .. } => UpdatedDescription,
        }
    }
}

impl Type<sqlx::Postgres> for TransactionEvent {
    fn type_info() -> <sqlx::Postgres as sqlx::Database>::TypeInfo {
        <&[u8] as Type<sqlx::Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, sqlx::Postgres> for TransactionEvent {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Postgres as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let bytes: Vec<u8> = postcard::to_allocvec(self)?;
        <&[u8] as Encode<sqlx::Postgres>>::encode(&bytes, buf)
    }
}

impl<'r> Decode<'r, sqlx::Postgres> for TransactionEvent {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let bytes = <&[u8] as Decode<sqlx::Postgres>>::decode(value)?;
        Ok(postcard::from_bytes::<TransactionEvent>(bytes)?)
    }
}

#[derive(Default, Serialize, Deserialize, Clone)]
#[allow(dead_code)]
pub struct TransactionState {
    pub id: Cuid,
    pub author: Cuid,
    pub description: String,
    pub updates: Vec<BalanceUpdate>,
    pub created_at: chrono::DateTime<Utc>,
}

impl TransactionState {
    #[allow(dead_code)]
    pub fn apply(&mut self, event: TransactionEvent) {
        match event {
            TransactionEvent::Created {
                id,
                author,
                description,
                updates,
                created_at,
            } => {
                self.id = id;
                self.author = author;
                self.description = description;
                self.updates = updates;
                self.created_at = created_at;
            }
            TransactionEvent::UpdatedDescription { new_desc, .. } => self.description = new_desc,
            TransactionEvent::UpdatedBalancedUpdates {
                new_balanceupdates, ..
            } => self.updates = new_balanceupdates,
        }
    }
}

#[cfg(test)]
mod test_transaction {
    use crate::{cuid::Cuid, journal::transaction::BalanceUpdate};
    use chrono::Utc;
    use sqlx::{PgPool, prelude::FromRow};

    use super::TransactionEvent;

    #[sqlx::test]
    async fn test_encode_decode_transactionevent(pool: PgPool) {
        let original_event = TransactionEvent::Created {
            id: Cuid::new10(),
            author: Cuid::new10(),
            description: "test".to_string(),
            updates: vec![BalanceUpdate {
                account_id: Cuid::new10(),
                changed_by: -45,
            }],
            created_at: Utc::now(),
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

        let event: TransactionEvent = sqlx::query_scalar(
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
            event: TransactionEvent,
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
