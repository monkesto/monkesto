pub mod commands;
pub mod views;

use crate::id::Ident;
use crate::journal::domain::{AccountEvent, JournalDomainEvent, TransactionEvent};
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
use crate::journal::account::AccountId;
use crate::journal::member::JournalMember;
use crate::journal::{Journal, Permissions, validate_permissions};
use crate::journal::{JournalError, JournalId};
use crate::status::Status;
use crate::time_provider::Timestamp;
use disintegrate::{Decision, StateMutate, StateQuery};
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Debug;
use std::fmt::Display;
use std::str::FromStr;
use thiserror::Error;

#[derive(Error, Debug, Serialize, Deserialize, PartialEq)]
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
    ImbalancedTransaction(Vec<BalanceUpdate>),
}

// TODO(gabriel) there's probably a more efficient way to validate that the applicable accounts exist
#[derive(StateQuery, Clone, Default, Serialize, Deserialize)]
#[state_query(AccountEvent)]
pub struct AllJournalAccounts {
    #[id]
    journal_id: JournalId,
    accounts: HashSet<AccountId>,
}

impl AllJournalAccounts {
    pub fn new(journal_id: JournalId) -> Self {
        Self {
            journal_id,
            ..Default::default()
        }
    }
}

impl StateMutate for AllJournalAccounts {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            AccountEvent::AccountCreated { account_id, .. } => _ = self.accounts.insert(account_id),
            AccountEvent::AccountRenamed { .. } => {}
            AccountEvent::AccountDeleted { account_id, .. } => {
                _ = self.accounts.remove(&account_id)
            }
        }
    }
}

#[derive(StateQuery, Clone, Default, Serialize, Deserialize)]
#[state_query(TransactionEvent)]
pub struct Transaction {
    #[id]
    transaction_id: TransactionId,
    journal_id: JournalId,
    updates: Vec<BalanceUpdate>,
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
                balance_updates,
                journal_id,
                ..
            } => {
                self.journal_id = journal_id;
                self.updates = balance_updates;
                self.status = Status::Valid;
            }
            TransactionEvent::TransactionDeleted { .. } => self.status = Status::Deleted,
        }
    }
}

pub struct CreateTransaction {
    transaction_id: TransactionId,
    journal_id: JournalId,
    entries: Vec<BalanceUpdate>,
    authority: Authority,
    timestamp: Timestamp,
}

impl CreateTransaction {
    pub fn new(
        transaction_id: TransactionId,
        journal_id: JournalId,
        entries: Vec<BalanceUpdate>,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            transaction_id,
            journal_id,
            entries,
            authority,
            timestamp,
        }
    }
}

impl Decision for CreateTransaction {
    type Event = JournalDomainEvent;
    type StateQuery = (Transaction, AllJournalAccounts, Journal, JournalMember);
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        (
            Transaction::new(self.transaction_id),
            AllJournalAccounts::new(self.journal_id),
            Journal::new(self.journal_id),
            JournalMember::new(
                self.journal_id,
                self.authority.user_id().unwrap_or_default(),
            ),
        )
    }

    fn process(
        &self,
        (transaction, accounts, journal, actor): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if transaction.status.found() {
            return Err(JournalError::TransactionIdCollision(self.transaction_id));
        }

        if !journal.status.valid() {
            return Err(JournalError::InvalidJournal(self.journal_id));
        }

        let mut balance = 0;

        for update in self.entries.iter() {
            if !accounts.accounts.contains(&update.account_id) {
                return Err(JournalError::InvalidAccount(update.account_id));
            }

            match update.entry_type {
                EntryType::Credit => balance += update.amount as i64,
                EntryType::Debit => balance -= update.amount as i64,
            }
        }

        if balance != 0 {
            return Err(JournalError::TransactionValidation(
                TransactionValidationError::ImbalancedTransaction(self.entries.clone()),
            ));
        }

        if !validate_permissions(
            actor,
            &self.authority,
            journal.owner,
            Permissions::APPEND_TRANSACTION,
        ) {
            return Err(JournalError::Permissions(Permissions::APPEND_TRANSACTION));
        }

        Ok(vec![JournalDomainEvent::TransactionCreated {
            transaction_id: self.transaction_id,
            journal_id: self.journal_id,
            balance_updates: self.entries.clone(),
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

pub struct DeleteTransaction {
    transaction_id: TransactionId,
    journal_id: JournalId,
    authority: Authority,
    timestamp: Timestamp,
}

#[expect(unused)]
impl DeleteTransaction {
    pub fn new(
        transaction_id: TransactionId,
        journal_id: JournalId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            transaction_id,
            journal_id,
            authority,
            timestamp,
        }
    }
}

impl Decision for DeleteTransaction {
    type Event = JournalDomainEvent;
    type StateQuery = (Transaction, Journal, JournalMember);
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        (
            Transaction::new(self.transaction_id),
            Journal::new(self.journal_id),
            JournalMember::new(
                self.journal_id,
                self.authority.user_id().unwrap_or_default(),
            ),
        )
    }

    fn process(
        &self,
        (transaction, journal, actor): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if !transaction.status.valid() || transaction.journal_id != self.journal_id {
            return Err(JournalError::InvalidTransaction(self.transaction_id));
        }

        if !journal.status.valid() {
            return Err(JournalError::InvalidJournal(self.journal_id));
        }

        if !validate_permissions(actor, &self.authority, journal.owner, Permissions::OWNER) {
            return Err(JournalError::Permissions(Permissions::OWNER));
        }

        Ok(vec![JournalDomainEvent::TransactionDeleted {
            transaction_id: self.transaction_id,
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Copy, Eq)]
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
    type Err = JournalError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Dr" => Ok(Self::Debit),
            "Cr" => Ok(Self::Credit),
            _ => Err(JournalError::TransactionValidation(
                TransactionValidationError::InvalidEntryType(s.to_string()),
            )),
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub struct BalanceUpdate {
    pub account_id: AccountId,
    pub amount: u64,
    pub entry_type: EntryType,
}
