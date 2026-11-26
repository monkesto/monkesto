pub mod commands;
pub mod entry;
pub mod memo;
pub mod views;

use crate::id::Ident;
use axum::Router;
use axum::routing::{get, post};
use axum_login::login_required;
use std::collections::HashSet;

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
use crate::journal::transaction::entry::TransactionEntries;
pub(crate) use crate::journal::transaction::entry::{EntryKind, EntrySide, TransactionEntry};
use crate::journal::transaction::memo::Memo;
use crate::journal::{JournalId, Permissions};
use crate::time::Timestamp;
use chrono::{DateTime, Datelike, Utc};
use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
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

pub struct TransactionState {
    pub id: TransactionId,
    #[expect(unused)]
    pub journal_id: JournalId,
    pub entries: TransactionEntries,
    pub memo: Option<Memo>,
}

impl JournalService {
    #[allow(clippy::too_many_arguments)]
    pub async fn create_transaction(
        &self,
        transaction_id: TransactionId,
        journal_id: JournalId,
        entries: TransactionEntries,
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

        let mut entry_ids = HashSet::new();

        for entry in &entries.0 {
            if !entry_ids.insert(entry.entry_id) {
                Err(TransactionValidationError::EntryIdCollision(entry.entry_id))?;
            }

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

        let event_id = append_creation_event_in_tx(
            &mut tx,
            JournalDomainEvent::TransactionCreated {
                transaction_id,
                journal_id,
                period,
                memo,
                entries,
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
            SELECT t.id as "id: TransactionId", t.journal_id as "journal_id: JournalId", t.memo as "memo: Memo", t.entries as "entries: TransactionEntries", e.authority as "authority: Authority", e.timestamp as "timestamp: Timestamp", e.display_timestamp as "display_timestamp: Timestamp"
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

            (TransactionState {
                id: t.id,
                journal_id,
                entries: t.entries,
                memo: t.memo,
            }, t.authority, timestamp)
        }).collect();

        Ok(transactions)
    }
}
