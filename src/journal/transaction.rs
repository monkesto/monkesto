use crate::{cuid::Cuid, known_errors::MonkestoResult};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Decode, Encode, Type, postgres::PgValueRef};

#[async_trait]
#[allow(dead_code)]
pub trait TransactionStore {
    /// creates a new transaction state in the event store with the data from the creation event
    ///
    /// it should return an error if the event passed in is not a creation event
    async fn create_transaction(&self, creation_event: TransactionEvent) -> MonkestoResult<()>;

    /// applies a TransactionEvent to the event store and updates the cached state
    async fn push_event(&self, transction_id: &Cuid, event: TransactionEvent)
    -> MonkestoResult<()>;

    async fn get_transaction(&self, transacion_id: &Cuid) -> MonkestoResult<TransactiionState>;
}

#[allow(dead_code)]
pub struct Transactions {
    store: dyn TransactionStore,
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
    },
    UpdatedBalancedUpdates {
        new_balanceupdates: Vec<BalanceUpdate>,
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

#[derive(Default, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct TransactiionState {
    pub id: Cuid,
    pub description: String,
    pub updates: Vec<BalanceUpdate>,
    pub created_at: chrono::DateTime<Utc>,
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
