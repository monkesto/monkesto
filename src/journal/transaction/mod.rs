pub mod commands;
pub mod entry;
pub mod memo;
pub mod views;

use crate::id::Ident;
use axum::Router;
use axum::routing::{get, post};
use axum_login::login_required;

id!(TransactionId, Ident::new16());

pub fn router() -> Router<crate::StateType> {
    Router::new()
        .route(
            "/journal/{id}/transaction",
            get(views::transaction_list_page),
        )
        .route("/journal/{id}/transaction", post(commands::transact))
        .route_layer(login_required!(crate::BackendType, login_url = "/signin"))
}

use crate::authority::Authority;
use crate::id;
use crate::journal::activity::scope::ActivityScope;
use crate::journal::error::{JournalError, JournalResult, TransactionValidationError};
use crate::journal::event::JournalDomainEvent;
use crate::journal::service::stream_lock::{
    append_creation_event_in_tx, assert_multiple_streams_exist,
};
use crate::journal::service::{EventId, EventType, JournalService, StreamType};
use crate::journal::transaction::entry::{EntryId, EntryKind, EntrySide};
use crate::journal::transaction::memo::Memo;
use crate::journal::{JournalId, Permissions};
use crate::time::Timestamp;
use chrono::{DateTime, Datelike, Utc};
use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, FromRow, Postgres, Type};
use std::fmt::Debug;
use thiserror::Error;

#[repr(i8)]
#[derive(Copy, Clone, Default, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum FinancialPeriod {
    #[default]
    January = 1,
    February,
    March,
    April,
    May,
    June,
    July,
    August,
    September,
    October,
    November,
    December,
}

#[derive(Debug, Error, PartialEq)]
#[error("{0}")]
pub struct FinancialPeriodFromIntError(pub i8);

impl From<DateTime<Utc>> for FinancialPeriod {
    fn from(timestamp: DateTime<Utc>) -> Self {
        Self::try_from(timestamp.month() as i8)
            .expect("chrono timestamps should always have a valid month")
    }
}

impl TryFrom<i8> for FinancialPeriod {
    type Error = FinancialPeriodFromIntError;

    fn try_from(value: i8) -> Result<Self, Self::Error> {
        match value {
            x if x == FinancialPeriod::January as i8 => Ok(FinancialPeriod::January),
            x if x == FinancialPeriod::February as i8 => Ok(FinancialPeriod::February),
            x if x == FinancialPeriod::March as i8 => Ok(FinancialPeriod::March),
            x if x == FinancialPeriod::April as i8 => Ok(FinancialPeriod::April),
            x if x == FinancialPeriod::May as i8 => Ok(FinancialPeriod::May),
            x if x == FinancialPeriod::June as i8 => Ok(FinancialPeriod::June),
            x if x == FinancialPeriod::July as i8 => Ok(FinancialPeriod::July),
            x if x == FinancialPeriod::August as i8 => Ok(FinancialPeriod::August),
            x if x == FinancialPeriod::September as i8 => Ok(FinancialPeriod::September),
            x if x == FinancialPeriod::October as i8 => Ok(FinancialPeriod::October),
            x if x == FinancialPeriod::November as i8 => Ok(FinancialPeriod::November),
            x if x == FinancialPeriod::December as i8 => Ok(FinancialPeriod::December),
            _ => Err(FinancialPeriodFromIntError(value)),
        }
    }
}

impl Type<Postgres> for FinancialPeriod {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&i16 as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for FinancialPeriod {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <i16 as Encode<Postgres>>::encode(*self as i16, buf)
    }
}

impl<'r> Decode<'r, Postgres> for FinancialPeriod {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let int16 = <i16 as Decode<Postgres>>::decode(value)?;
        Ok(Self::try_from(int16 as i8)?)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TransactionEntry {
    pub amount: u64,
    pub entry_side: EntrySide,
    pub entry_kind: EntryKind,
}

#[derive(Copy, Clone, Serialize, Deserialize, Debug, PartialEq, FromRow)]
pub struct PersistedTransactionEntry {
    pub id: EntryId,
    pub amount: i64,
    pub entry_side: EntrySide,
    pub entry_kind: EntryKind,
}

pub struct TransactionState {
    pub id: TransactionId,
    #[expect(unused)]
    pub journal_id: JournalId,
    pub entries: Vec<PersistedTransactionEntry>,
    pub memo: Option<Memo>,
}

impl JournalService {
    #[allow(clippy::too_many_arguments)]
    pub async fn create_transaction(
        &self,
        transaction_id: TransactionId,
        journal_id: JournalId,
        entries: Vec<TransactionEntry>,
        period: FinancialPeriod,
        memo: Option<Memo>,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(
            &mut tx,
            journal_id,
            authority,
            Permissions::APPEND_TRANSACTION,
        )
        .await?;

        let mut account_ids = Vec::new();
        let mut activity_ids = Vec::new();
        let mut fund_ids = Vec::new();
        let mut account_and_external_balance = 0_i128;
        let mut internal_transfer_balance = 0_i128;

        for entry in &entries {
            let amount = i128::from(entry.amount);
            let change = match entry.entry_side {
                EntrySide::Debit => -amount,
                EntrySide::Credit => amount,
            };
            match entry.entry_kind {
                EntryKind::Account { account_id } => {
                    account_ids.push(*account_id);
                    account_and_external_balance += change;
                }
                EntryKind::Activity {
                    activity_id,
                    fund_id,
                    scope,
                } => {
                    activity_ids.push(*activity_id);
                    fund_ids.push(*fund_id);
                    match scope {
                        ActivityScope::Internal => internal_transfer_balance += change,
                        ActivityScope::External => account_and_external_balance += change,
                    }
                }
            }
        }

        if account_and_external_balance != 0 || internal_transfer_balance != 0 {
            return Err(TransactionValidationError::ImbalancedTransaction.into());
        }

        assert_multiple_streams_exist(&mut tx, journal_id, &account_ids, StreamType::Account)
            .await?;

        assert_multiple_streams_exist(&mut tx, journal_id, &activity_ids, StreamType::Activity)
            .await?;

        assert_multiple_streams_exist(&mut tx, journal_id, &fund_ids, StreamType::Fund).await?;

        for entry in entries {
            append_creation_event_in_tx(
                &mut tx,
                JournalDomainEvent::EntryCreated {
                    entry_id: EntryId::new(),
                    journal_id,
                    transaction_id,
                    amount: entry.amount,
                    entry_side: entry.entry_side,
                    entry_kind: entry.entry_kind,
                },
                None,
                authority,
                timestamp,
                None,
            )
            .await?;
        }

        let event_id = append_creation_event_in_tx(
            &mut tx,
            JournalDomainEvent::TransactionCreated {
                transaction_id,
                journal_id,
                financial_period: period,
                memo,
            },
            None,
            authority,
            timestamp,
            None,
        )
        .await?;

        tx.commit().await?;
        Ok(event_id)
    }

    pub async fn list_journal_transactions(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(TransactionState, Authority, Timestamp)>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(JournalError::Permissions(Permissions::READ));
        }

        // TODO(Gabriel): pagination
        let transactions = sqlx::query!(
            r#"
            SELECT t.id as "id: TransactionId", t.journal_id as "journal_id: JournalId", t.memo as "memo: Memo", e.authority as "authority: Authority", e.timestamp as "timestamp: Timestamp", e.display_timestamp as "display_timestamp: Timestamp",
                   ARRAY(
                        SELECT (en.id, en.amount, en.entry_side, en.entry_kind)
                        FROM entries en
                        WHERE en.transaction_id = t.id AND t.journal_id = $1
                   ) AS "entries!: Vec<(EntryId, i64, EntrySide, EntryKind)>"
            FROM transactions t
            INNER JOIN journal_events e
                ON e.stream_id = t.id AND t.journal_id = $1 AND e.stream_type = $2 AND e.event_type = $3
            WHERE t.journal_id = $1
            ORDER BY COALESCE(e.display_timestamp, e.timestamp) DESC
            "#,
            journal_id as JournalId,
            StreamType::Transaction as i16,
            EventType::TransactionCreated as i16,
        )
            .fetch_all(self.pool())
            .await?.into_iter().map(|t| {
            let timestamp = t.display_timestamp.unwrap_or(t.timestamp);

            let entries = t.entries.into_iter().map(|(id, amount, entry_side, entry_kind)| PersistedTransactionEntry {
                id,
                amount,
                entry_side,
                entry_kind,
            }).collect();

            (TransactionState {
                id: t.id,
                journal_id,
                entries,
                memo: t.memo,
            }, t.authority, timestamp)
        }).collect();

        Ok(transactions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authn::UserId;
    use crate::authority::Actor;
    use crate::journal::account::{AccountId, AccountType};
    use crate::journal::activity::{ActivityId, ActivityKind};
    use crate::journal::file::ObjectStore;
    use crate::journal::fund::FundId;
    use crate::journal::projection;
    use crate::name::Name;
    use sqlx::PgPool;
    use tokio::sync::broadcast;

    #[sqlx::test(migrations = "./migrations")]
    #[ignore = "requires a PostgreSQL test server with database creation privileges"]
    async fn transactions_with_offsetting_balance_groups_do_not_commit(
        pool: PgPool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        projection::install_sql(&pool).await?;
        let service = JournalService {
            pool,
            object_store: ObjectStore::Local {
                storage_directory: std::env::temp_dir(),
            },
            projection_applications: broadcast::channel(1).0,
            jewel_imports: Default::default(),
        };
        let owner = UserId::new();
        let authority = Authority::Direct(Actor::User(owner));
        let timestamp = Timestamp(DateTime::UNIX_EPOCH);
        let journal_id = JournalId::new();
        let account_id = AccountId::new();
        let activity_id = ActivityId::new();
        let fund_id = "gfund".parse::<FundId>()?;
        service
            .create_journal(journal_id, owner, Name::default(), authority, timestamp)
            .await?;
        service
            .create_account(
                account_id,
                journal_id,
                Name::default(),
                AccountType::Asset,
                authority,
                timestamp,
            )
            .await?;
        service
            .create_activity(
                activity_id,
                journal_id,
                Name::default(),
                ActivityKind::Income,
                authority,
                timestamp,
            )
            .await?;

        let debit = TransactionEntry {
            amount: 100,
            entry_side: EntrySide::Debit,
            entry_kind: EntryKind::Account { account_id },
        };
        let transfer_credit = TransactionEntry {
            amount: 100,
            entry_side: EntrySide::Credit,
            entry_kind: EntryKind::Activity {
                activity_id,
                fund_id,
                scope: ActivityScope::Internal,
            },
        };
        let transaction_id = TransactionId::new();
        let result = service
            .create_transaction(
                transaction_id,
                journal_id,
                vec![debit.clone(), transfer_credit.clone()],
                FinancialPeriod::January,
                None,
                authority,
                timestamp,
            )
            .await;
        assert!(matches!(
            result,
            Err(JournalError::TransactionValidation(
                TransactionValidationError::ImbalancedTransaction
            ))
        ));

        // The corrected transaction can reuse the ID only if the first write did not commit.
        service
            .create_transaction(
                transaction_id,
                journal_id,
                vec![
                    TransactionEntry {
                        entry_side: EntrySide::Debit,
                        ..transfer_credit.clone()
                    },
                    transfer_credit,
                    TransactionEntry {
                        entry_side: EntrySide::Credit,
                        ..debit.clone()
                    },
                    debit,
                ],
                FinancialPeriod::January,
                None,
                authority,
                timestamp,
            )
            .await?;
        Ok(())
    }
}
