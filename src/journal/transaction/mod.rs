pub mod commands;
pub mod views;

use crate::id::Ident;
use crate::journal::domain::{AFTEvent, JournalDomainEvent, TransactionEvent};
use axum::Router;
use axum::routing::{get, post};
use axum_login::login_required;
use std::collections::{HashMap, HashSet};

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
use crate::event_id::GetEventId;
use crate::id;
use crate::journal::account::{AccountId, AccountType};
use crate::journal::activity::{ActivityId, ActivityType};
use crate::journal::entry::{EntryId, EntryKind, EntrySide};
use crate::journal::fund::FundId;
use crate::journal::member::JournalMember;
use crate::journal::{Journal, JournalResult, JournalService, Permissions, validate_permissions};
use crate::journal::{JournalError, JournalId};
use crate::status::Status;
use crate::time_provider::Timestamp;
use disintegrate::{Decision, DecisionError, StateMutate, StateQuery};
use disintegrate_postgres::PgEventId;
use prost::Message;
use proto::event::journal::ProtoJournalDomainEvent;
use proto::transaction_entry::ProtoRepeatedTransactionEntryIds;
use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, FromRow, Postgres, Type};
use std::fmt::Debug;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum TransactionValidationError {
    #[error("Received an invalid entry type. Expected Dr or Cr, found {0}")]
    InvalidEntryType(String),
    #[error("Did not receive any transaction entries")]
    NoTransactionEntries,
    #[error("Did not receive a corresponding amount for an entry")]
    MissingEntryAmount,
    #[error("Did not receive a corresponding entry type for an entry")]
    MissingEntryType,
    #[error("Invalid entry amount: {0}")]
    ParseDecimal(String),
    #[error("Received an entry with a partial cent value: {0}")]
    PartialCentValue(String),
    #[error("Received an entry with a value greater than 9 quintillion")]
    OutOfRange(String),
    #[error(
        "Received an entry with a negative amount: {0}. Please use the debit/credit selector instead."
    )]
    NegativeEntryAmount(String),
    #[error("Imbalanced transaction: {:?}", 0)]
    ImbalancedTransaction(TransactionEntries),
    #[error(
        "attempted a transfer involving activity {0}, but it doesn't have a 'transfer' activity type"
    )]
    TransferViolation(ActivityId),
}

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

// TODO(gabriel) there's probably a more efficient way to validate that the applicable accounts exist
#[derive(StateQuery, Clone, Default, Serialize, Deserialize)]
#[state_query(AFTEvent)]
pub struct JournalAFTs {
    #[id]
    journal_id: JournalId,
    accounts: HashMap<AccountId, AccountType>,
    funds: HashSet<FundId>,
    activities: HashMap<ActivityId, ActivityType>,
}

impl JournalAFTs {
    pub fn new(journal_id: JournalId) -> Self {
        Self {
            journal_id,
            ..Default::default()
        }
    }
}

impl StateMutate for JournalAFTs {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            AFTEvent::AccountCreated {
                account_id,
                account_type,
                ..
            } => _ = self.accounts.insert(account_id, account_type),
            AFTEvent::AccountDeleted { account_id, .. } => _ = self.accounts.remove(&account_id),
            AFTEvent::FundCreated { fund_id, .. } => _ = self.funds.insert(fund_id),
            AFTEvent::ActivityCreated {
                activity_id,
                activity_type,
                ..
            } => _ = self.activities.insert(activity_id, activity_type),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct TransactionEntry {
    pub amount: u64,
    pub entry_side: EntrySide,
    pub entry_kind: EntryKind,
}

#[derive(StateQuery, Clone, Default, Serialize, Deserialize)]
#[state_query(TransactionEvent)]
pub struct Transaction {
    #[id]
    transaction_id: TransactionId,
    #[id]
    journal_id: JournalId,
    entries: TransactionEntryIds,
    status: Status,
}

impl Transaction {
    fn new(transaction_id: TransactionId) -> Self {
        Self {
            transaction_id,
            ..Default::default()
        }
    }
}

impl StateMutate for Transaction {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            TransactionEvent::TransactionCreated {
                entries,
                journal_id,
                ..
            } => {
                self.journal_id = journal_id;
                self.entries = entries;
                self.status = Status::Valid;
            }
        }
    }
}

pub struct CreateTransaction {
    transaction_id: TransactionId,
    journal_id: JournalId,
    entries: TransactionEntries,
    period: FinancialPeriod,
    authority: Authority,
    timestamp: Timestamp,
}

impl CreateTransaction {
    pub fn new(
        transaction_id: TransactionId,
        journal_id: JournalId,
        entries: TransactionEntries,
        period: FinancialPeriod,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            transaction_id,
            journal_id,
            entries,
            period,
            authority,
            timestamp,
        }
    }
}

impl Decision for CreateTransaction {
    type Event = JournalDomainEvent;
    type StateQuery = (Transaction, JournalAFTs, Journal, JournalMember);
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        (
            Transaction::new(self.transaction_id),
            JournalAFTs::new(self.journal_id),
            Journal::new(self.journal_id),
            JournalMember::new(
                self.journal_id,
                self.authority.user_id().unwrap_or_default(),
            ),
        )
    }

    fn process(
        &self,
        (transaction, afts, journal, actor): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if transaction.status.found() {
            return Err(JournalError::TransactionIdCollision(self.transaction_id));
        }

        if !journal.status.valid() {
            return Err(JournalError::InvalidJournal(self.journal_id));
        }

        if !validate_permissions(
            actor,
            self.authority,
            journal.owner,
            Permissions::APPEND_TRANSACTION,
        ) {
            return Err(JournalError::Permissions(Permissions::APPEND_TRANSACTION));
        }

        let mut overall_balance = 0;
        let mut transfer_balance = 0;
        let mut entry_ids = Vec::with_capacity(self.entries.0.len());
        let mut events = Vec::with_capacity(self.entries.0.len() + 1);

        for entry in self.entries.0.iter() {
            let entry_change = match entry.entry_side {
                EntrySide::Credit => entry.amount as i64,
                EntrySide::Debit => -(entry.amount as i64),
            };

            overall_balance += entry_change;

            match entry.entry_kind {
                EntryKind::Account { account_id } => {
                    if !afts.accounts.contains_key(&account_id) {
                        return Err(JournalError::InvalidAccount(account_id));
                    }
                }
                EntryKind::Activity {
                    activity_id,
                    fund_id,
                    transfer,
                } => {
                    if !afts.funds.contains(&fund_id) {
                        return Err(JournalError::InvalidActivity(activity_id));
                    }

                    if let Some(activity_type) = afts.activities.get(&activity_id) {
                        if transfer {
                            if *activity_type != ActivityType::Transfer {
                                return Err(JournalError::TransactionValidation(
                                    TransactionValidationError::TransferViolation(activity_id),
                                ));
                            }
                            transfer_balance += entry_change;
                        }
                    } else {
                        return Err(JournalError::InvalidActivity(activity_id));
                    }
                }
            }

            // TODO(Gabriel): Check for entry id collisions
            let entry_id = EntryId::new();
            entry_ids.push(entry_id);
            events.push(JournalDomainEvent::EntryCreated {
                entry_id,
                journal_id: self.journal_id,
                amount: entry.amount,
                entry_side: entry.entry_side,
                entry_kind: entry.entry_kind,
                authority: self.authority,
                timestamp: self.timestamp,
            })
        }

        if transfer_balance != 0 || overall_balance != 0 {
            return Err(JournalError::TransactionValidation(
                TransactionValidationError::ImbalancedTransaction(self.entries.clone()),
            ));
        }

        events.push(JournalDomainEvent::TransactionCreated {
            transaction_id: self.transaction_id,
            journal_id: self.journal_id,
            entries: TransactionEntryIds(entry_ids),
            financial_period: self.period,
            authority: self.authority,
            timestamp: self.timestamp,
        });

        Ok(events)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Default, Serialize, Deserialize)]
pub struct TransactionEntries(pub Vec<TransactionEntry>);

#[derive(Debug, PartialEq, Eq, Clone, Default, Serialize, Deserialize)]
pub struct TransactionEntryIds(pub Vec<EntryId>);

impl Type<Postgres> for TransactionEntryIds {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&[u8] as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for TransactionEntryIds {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes = ProtoRepeatedTransactionEntryIds::from(TransactionEntryIds(self.0.clone()))
            .encode_to_vec();
        <Vec<u8> as Encode<Postgres>>::encode(bytes, buf)
    }
}

impl<'r> Decode<'r, Postgres> for TransactionEntryIds {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes = <&[u8] as Decode<Postgres>>::decode(value)?;
        let prost_entries = ProtoRepeatedTransactionEntryIds::decode(bytes)?;
        Ok(prost_entries.try_into()?)
    }
}

#[derive(Copy, Clone, Serialize, Deserialize, Debug, PartialEq, FromRow)]
pub struct StoredTransactionEntry {
    pub id: EntryId,
    pub journal_id: JournalId,
    pub amount: i64,
    pub entry_side: EntrySide,
    pub entry_kind: EntryKind,
}

pub struct TransactionState {
    pub id: TransactionId,
    #[expect(unused)]
    pub journal_id: JournalId,
    pub entries: Vec<StoredTransactionEntry>,
}

#[derive(FromRow)]
struct TransactionStateWithPayload {
    id: TransactionId,
    #[expect(unused)]
    journal_id: JournalId,
    entries: TransactionEntryIds,
    payload: Vec<u8>,
}

impl JournalService {
    pub async fn create_transaction(
        &self,
        transaction_id: TransactionId,
        journal_id: JournalId,
        entries: Vec<TransactionEntry>,
        period: FinancialPeriod,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(CreateTransaction::new(
                transaction_id,
                journal_id,
                TransactionEntries(entries),
                period,
                authority,
                timestamp,
            ))
            .await?
            .event_id())
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

        let transactions = sqlx::query_as!(
            TransactionStateWithPayload,
            r#"
            SELECT t.id as "id: TransactionId", t.journal_id as "journal_id: JournalId", t.entries as "entries: TransactionEntryIds", e.payload as "payload!"
            FROM transactions t
            INNER JOIN event e
                ON e.transaction_id = t.id AND e.event_type = 'TransactionCreated'
            WHERE t.journal_id = $1
            "#,
            journal_id as JournalId)
            .fetch_all(&self.projection_pool)
            .await?;

        let mut transactions_with_meta = Vec::with_capacity(transactions.len());

        // TODO(Gabriel) bundling all journal entries for now, probably will want to load them lazily at some point
        let entries: HashMap<EntryId, StoredTransactionEntry> = sqlx::query_as!(
            StoredTransactionEntry,
            r#"
            SELECT id as "id: EntryId", journal_id as "journal_id: JournalId", amount, entry_side as "entry_side: EntrySide", entry_kind as "entry_kind: EntryKind" from transaction_entries WHERE journal_id = $1
            "#,
            journal_id as JournalId
        ).fetch_all(&self.projection_pool)
            .await?
            .into_iter()
            .map(|e| (e.id, e))
            .collect();

        for transaction in transactions {
            let payload = JournalDomainEvent::try_from(ProtoJournalDomainEvent::decode(
                transaction.payload.as_slice(),
            )?)?;

            let mut tx_entries = Vec::new();

            for entry_id in transaction.entries.0 {
                if let Some(entry) = entries.get(&entry_id) {
                    tx_entries.push(*entry)
                } else {
                    return Err(JournalError::InvalidEntry(entry_id));
                }
            }

            match payload {
                JournalDomainEvent::TransactionCreated {
                    authority,
                    timestamp,
                    ..
                } => {
                    transactions_with_meta.push((
                        TransactionState {
                            id: transaction.id,
                            journal_id,
                            entries: tx_entries,
                        },
                        authority,
                        timestamp,
                    ));
                }
                _ => unreachable!("TransactionCreated events are filtered by the sql query"),
            }
        }

        Ok(transactions_with_meta)
    }
}
