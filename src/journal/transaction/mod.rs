use std::fmt::Display;
use std::str::FromStr;
use std::sync::Arc;

use crate::authority::UserId;
use crate::ident::AccountId;
use crate::ident::TransactionId;
use crate::known_errors::KnownErrors;
use crate::known_errors::MonkestoResult;

use async_trait::async_trait;
use chrono::DateTime;
use chrono::Utc;
use dashmap::DashMap;
use serde::Deserialize;
use serde::Serialize;
use sqlx::Decode;
use sqlx::Encode;
use sqlx::Type;
use sqlx::postgres::PgValueRef;

#[async_trait]
#[expect(dead_code)]
pub trait TransactionStore: Clone + Send + Sync + 'static {
    /// creates a new transaction state in the event store with the data from the creation event
    ///
    /// it should return an error if the event passed in is not a creation event
    async fn create_transaction(&self, creation_event: TransactionEvent) -> MonkestoResult<()>;

    /// applies a TransactionEvent to the event store and updates the cached state
    async fn push_event(
        &self,
        transaction_id: &TransactionId,
        event: TransactionEvent,
    ) -> MonkestoResult<()>;

    async fn get_transaction(
        &self,
        transaction_id: &TransactionId,
    ) -> MonkestoResult<TransactionState>;

    async fn seed_transaction(
        &self,
        creation_event: TransactionEvent,
        update_events: Vec<TransactionEvent>,
    ) -> MonkestoResult<()> {
        if let TransactionEvent::Created { id, .. } = creation_event {
            self.create_transaction(creation_event).await?;

            for event in update_events {
                self.push_event(&id, event).await?;
            }
        } else {
            return Err(KnownErrors::IncorrectEventType);
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct TransasctionMemoryStore {
    events: Arc<DashMap<TransactionId, Vec<TransactionEvent>>>,
    transaction_table: Arc<DashMap<TransactionId, TransactionState>>,
}

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
            ref updates,
            created_at,
        } = creation_event
        {
            self.transaction_table.insert(
                id,
                TransactionState {
                    id,
                    author,
                    updates: updates.clone(),
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
        transaction_id: &TransactionId,
        event: TransactionEvent,
    ) -> MonkestoResult<()> {
        if let Some(mut events) = self.events.get_mut(transaction_id)
            && let Some(mut state) = self.transaction_table.get_mut(transaction_id)
        {
            events.push(event.clone());
            state.apply(event);
        }

        Ok(())
    }

    async fn get_transaction(
        &self,
        transaction_id: &TransactionId,
    ) -> MonkestoResult<TransactionState> {
        self.transaction_table
            .get(transaction_id)
            .map(|s| (*s).clone())
            .ok_or(KnownErrors::TransactionDoesntExist)
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
    type Err = KnownErrors;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Dr" => Ok(Self::Debit),
            "Cr" => Ok(Self::Credit),
            _ => Err(KnownErrors::InvalidInput),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct BalanceUpdate {
    pub account_id: AccountId,
    pub amount: u64,
    pub entry_type: EntryType,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum TransactionEvent {
    Created {
        id: TransactionId,
        author: UserId,
        updates: Vec<BalanceUpdate>,
        created_at: DateTime<Utc>,
    },
    UpdatedBalancedUpdates {
        new_balanceupdates: Vec<BalanceUpdate>,
        updater: UserId,
    },
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "smallint")]
#[repr(i16)]
pub enum TransactionEventType {
    Created = 1,
    UpdatedDescription = 2,
    UpdatedBalanceUpdates = 3,
}

impl TransactionEvent {
    #[expect(dead_code)]
    fn get_type(&self) -> TransactionEventType {
        use TransactionEventType::*;
        match self {
            Self::Created { .. } => Created,
            Self::UpdatedBalancedUpdates { .. } => UpdatedBalanceUpdates,
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

#[derive(Serialize, Deserialize, Clone)]
pub struct TransactionState {
    pub id: TransactionId,
    pub author: UserId,
    pub updates: Vec<BalanceUpdate>,
    pub created_at: chrono::DateTime<Utc>,
}

impl TransactionState {
    pub fn apply(&mut self, event: TransactionEvent) {
        match event {
            TransactionEvent::Created {
                id,
                author,
                updates,
                created_at,
            } => {
                self.id = id;
                self.author = author;
                self.updates = updates;
                self.created_at = created_at;
            }
            TransactionEvent::UpdatedBalancedUpdates {
                new_balanceupdates, ..
            } => self.updates = new_balanceupdates,
        }
    }
}

#[cfg(test)]
mod test_transaction {
    use crate::authority::UserId;
    use crate::ident::AccountId;
    use crate::ident::TransactionId;
    use crate::journal::transaction::BalanceUpdate;
    use crate::journal::transaction::EntryType;
    use chrono::Utc;
    use sqlx::PgPool;
    use sqlx::prelude::FromRow;

    use super::TransactionEvent;

    #[sqlx::test]
    async fn test_encode_decode_transactionevent(pool: PgPool) {
        let original_event = TransactionEvent::Created {
            id: TransactionId::new(),
            author: UserId::new(),
            updates: vec![BalanceUpdate {
                account_id: AccountId::new(),
                amount: 45,
                entry_type: EntryType::Debit,
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
