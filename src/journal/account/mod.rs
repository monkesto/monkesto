pub mod commands;
pub mod views;

use axum::Router;
use axum::routing::get;
use axum_login::login_required;
use std::convert::From;

pub fn router() -> Router<crate::StateType> {
    Router::new()
        .route("/journal/{id}/account", get(views::account_list_page))
        .route(
            "/journal/{id}/createaccount",
            axum::routing::post(commands::create_account),
        )
        .route_layer(login_required!(crate::BackendType, login_url = "/signin"))
}

use crate::authority::Authority;
use crate::id;
use crate::id::Ident;
use crate::journal::domain::{AccountEvent, JournalDomainEvent};
use crate::journal::member::JournalMember;
use crate::journal::{Journal, Permissions, validate_permissions};
use crate::journal::{JournalError, JournalId};
use crate::name::Name;
use crate::status::Status;
use crate::time_provider::Timestamp;
use disintegrate::{Decision, StateMutate, StateQuery};
use serde::Deserialize;
use serde::Serialize;

id!(AccountId, Ident::new16());

#[derive(StateQuery, Clone, Default, Serialize, Deserialize)]
#[state_query(AccountEvent)]
pub struct Account {
    #[id]
    account_id: AccountId,
    journal_id: JournalId,
    name: Name,
    status: Status,
}

impl StateMutate for Account {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            AccountEvent::AccountCreated {
                name, journal_id, ..
            } => {
                self.journal_id = journal_id;
                self.name = name;
                self.status = Status::Valid;
            }
            AccountEvent::AccountRenamed { new_name, .. } => {
                self.name = new_name;
            }
            AccountEvent::AccountDeleted { .. } => {
                self.status = Status::Deleted;
            }
        }
    }
}

impl Account {
    fn new(account_id: AccountId) -> Self {
        Self {
            account_id,
            ..Default::default()
        }
    }
}

pub struct CreateAccount {
    account_id: AccountId,
    journal_id: JournalId,
    name: Name,
    authority: Authority,
    timestamp: Timestamp,
}

impl CreateAccount {
    pub fn new(
        account_id: AccountId,
        journal_id: JournalId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            account_id,
            journal_id,
            name,
            authority,
            timestamp,
        }
    }
}

impl Decision for CreateAccount {
    type Event = JournalDomainEvent;
    type StateQuery = (Account, Journal, JournalMember);
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        (
            Account::new(self.account_id),
            Journal::new(self.journal_id),
            JournalMember::new(
                self.journal_id,
                self.authority.user_id().unwrap_or_default(),
            ),
        )
    }

    fn process(
        &self,
        (account, journal, actor): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if account.status.found() {
            return Err(JournalError::AccountIdCollision(self.account_id));
        }

        if !journal.status.valid() {
            return Err(JournalError::InvalidJournal(self.journal_id));
        }

        if !validate_permissions(
            actor,
            &self.authority,
            journal.owner,
            Permissions::ADD_ACCOUNT,
        ) {
            return Err(JournalError::Permissions(Permissions::ADD_ACCOUNT));
        }

        Ok(vec![JournalDomainEvent::AccountCreated {
            account_id: self.account_id,
            journal_id: self.journal_id,
            name: self.name.clone(),
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

pub struct RenameAccount {
    account_id: AccountId,
    journal_id: JournalId,
    name: Name,
    authority: Authority,
    timestamp: Timestamp,
}

#[expect(unused)]
impl RenameAccount {
    pub fn new(
        account_id: AccountId,
        journal_id: JournalId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            account_id,
            journal_id,
            name,
            authority,
            timestamp,
        }
    }
}

impl Decision for RenameAccount {
    type Event = JournalDomainEvent;
    type StateQuery = (Account, Journal, JournalMember);
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        (
            Account::new(self.account_id),
            Journal::new(self.journal_id),
            JournalMember::new(
                self.journal_id,
                self.authority.user_id().unwrap_or_default(),
            ),
        )
    }

    fn process(
        &self,
        (account, journal, actor): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if !account.status.valid() || account.journal_id != self.journal_id {
            return Err(JournalError::InvalidAccount(self.account_id));
        }

        if !journal.status.valid() {
            return Err(JournalError::InvalidJournal(self.journal_id));
        }

        if !validate_permissions(actor, &self.authority, journal.owner, Permissions::OWNER) {
            return Err(JournalError::Permissions(Permissions::OWNER));
        }

        Ok(vec![JournalDomainEvent::AccountRenamed {
            account_id: self.account_id,
            new_name: self.name.clone(),
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

pub struct DeleteAccount {
    account_id: AccountId,
    journal_id: JournalId,
    authority: Authority,
    timestamp: Timestamp,
}

#[expect(unused)]
impl DeleteAccount {
    pub fn new(
        account_id: AccountId,
        journal_id: JournalId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            account_id,
            journal_id,
            authority,
            timestamp,
        }
    }
}

impl Decision for DeleteAccount {
    type Event = JournalDomainEvent;
    type StateQuery = (Account, Journal, JournalMember);
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        (
            Account::new(self.account_id),
            Journal::new(self.journal_id),
            JournalMember::new(
                self.journal_id,
                self.authority.user_id().unwrap_or_default(),
            ),
        )
    }

    fn process(
        &self,
        (account, journal, actor): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if !account.status.valid() || account.journal_id != self.journal_id {
            return Err(JournalError::InvalidAccount(self.account_id));
        }

        if !journal.status.valid() {
            return Err(JournalError::InvalidJournal(self.journal_id));
        }

        if !validate_permissions(actor, &self.authority, journal.owner, Permissions::OWNER) {
            return Err(JournalError::Permissions(Permissions::OWNER));
        }

        Ok(vec![JournalDomainEvent::AccountDeleted {
            account_id: self.account_id,
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}
