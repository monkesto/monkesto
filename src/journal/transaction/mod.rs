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
use crate::journal::account::{AccountError, AccountId};
use crate::journal::member::JournalMember;
use crate::journal::transaction::TransactionError::InvalidEntryType;
use crate::journal::{Journal, Permissions, validate_permissions};
use crate::journal::{JournalError, JournalId};
use crate::status::Status;
use crate::time_provider::Timestamp;
use TransactionError::InvalidTransaction;
use disintegrate::{Decision, StateMutate, StateQuery};
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Debug;
use std::fmt::Display;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize, PartialEq)]
pub enum TransactionError {
    #[error("Invalid transaction: {0}")]
    InvalidTransaction(TransactionId),

    #[error("A transaction with the id {0} already exists")]
    IdCollision(TransactionId),

    #[error("Invalid journal: {0}")]
    InvalidJournal(JournalId),

    #[error("Invalid account: {0}")]
    InvalidAccount(AccountId),

    #[error("Invalid entry type: {0} expected \"Dr\" or \"Cr\"")]
    InvalidEntryType(String),

    #[error("Failed to parse a decimal number from the input: {0}")]
    ParseDecimal(String),

    #[error("The transaction does not have balanced credits and debits")]
    TransactionNotBalanced,

    #[error("The journal store returned an error: {0}")]
    JournalStore(#[from] JournalError),

    #[error("The account store returned an error: {0}")]
    AccountStore(#[from] AccountError),

    #[error("The user does not have the required permission: {:?}", .0)]
    Permission(Permissions),

    #[error("The supplied balance updates were invalid: {0}")]
    InvalidBalanceUpdates(String),
}

#[expect(unused)]
pub type TransactionResult<T> = Result<T, TransactionError>;

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
    type Error = TransactionError;

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
            return Err(TransactionError::IdCollision(self.transaction_id));
        }

        if !journal.status.valid() {
            return Err(TransactionError::InvalidJournal(self.journal_id));
        }

        let mut balance = 0;

        for update in self.entries.iter() {
            if !accounts.accounts.contains(&update.account_id) {
                return Err(TransactionError::InvalidAccount(update.account_id));
            }

            match update.entry_type {
                EntryType::Credit => balance += update.amount as i64,
                EntryType::Debit => balance -= update.amount as i64,
            }
        }

        if balance != 0 {
            return Err(TransactionError::TransactionNotBalanced);
        }

        if !validate_permissions(
            actor,
            &self.authority,
            journal.owner,
            Permissions::APPEND_TRANSACTION,
        ) {
            return Err(TransactionError::Permission(
                Permissions::APPEND_TRANSACTION,
            ));
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
    type Error = TransactionError;

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
            return Err(InvalidTransaction(self.transaction_id));
        }

        if !journal.status.valid() {
            return Err(TransactionError::InvalidJournal(self.journal_id));
        }

        if !validate_permissions(actor, &self.authority, journal.owner, Permissions::OWNER) {
            return Err(TransactionError::Permission(Permissions::OWNER));
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
    type Err = TransactionError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Dr" => Ok(Self::Debit),
            "Cr" => Ok(Self::Credit),
            _ => Err(InvalidEntryType(s.to_string())),
        }
    }
}

#[derive(Serialize, Deserialize, Copy, Clone, Debug, PartialEq, Eq)]
pub struct BalanceUpdate {
    pub account_id: AccountId,
    pub amount: u64,
    pub entry_type: EntryType,
}
