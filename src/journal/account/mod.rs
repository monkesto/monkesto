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
use crate::event_id::GetEventId;
use crate::id;
use crate::id::Ident;
use crate::journal::domain::{AccountEvent, JournalDomainEvent};
use crate::journal::member::JournalMember;
use crate::journal::{Journal, JournalResult, JournalService, Permissions, validate_permissions};
use crate::journal::{JournalError, JournalId};
use crate::name::Name;
use crate::status::Status;
use crate::time_provider::Timestamp;
use disintegrate::{Decision, DecisionError, StateMutate, StateQuery};
use disintegrate_postgres::PgEventId;
use prost::Message;
use proto::event::journal::ProtoJournalDomainEvent;
use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, FromRow, Postgres, Type};
use thiserror::Error;

id!(AccountId, Ident::new16());

#[repr(i8)]
#[derive(Copy, Clone, Default, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum AccountType {
    #[default]
    Asset = 1,
    Liability = 2,
}

#[derive(Debug, Error, PartialEq)]
#[error("{0}")]
pub struct AccountTypeFromIntError(pub i8);

impl TryFrom<i8> for AccountType {
    type Error = AccountTypeFromIntError;

    fn try_from(value: i8) -> Result<Self, Self::Error> {
        match value {
            x if x == AccountType::Asset as i8 => Ok(AccountType::Asset),
            x if x == AccountType::Liability as i8 => Ok(AccountType::Liability),
            _ => Err(AccountTypeFromIntError(value)),
        }
    }
}

impl Type<Postgres> for AccountType {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&i16 as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for AccountType {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <i16 as Encode<Postgres>>::encode(*self as i16, buf)
    }
}

impl<'r> Decode<'r, Postgres> for AccountType {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let int16 = <i16 as Decode<Postgres>>::decode(value)?;
        Ok(Self::try_from(int16 as i8)?)
    }
}

#[derive(StateQuery, Clone, Default, Serialize, Deserialize)]
#[state_query(AccountEvent)]
pub struct Account {
    #[id]
    account_id: AccountId,
    journal_id: JournalId,
    name: Name,
    account_type: AccountType,
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
    account_type: AccountType,
    authority: Authority,
    timestamp: Timestamp,
}

impl CreateAccount {
    pub fn new(
        account_id: AccountId,
        journal_id: JournalId,
        name: Name,
        account_type: AccountType,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            account_id,
            journal_id,
            name,
            account_type,
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
            self.authority,
            journal.owner,
            Permissions::ADD_ACCOUNT,
        ) {
            return Err(JournalError::Permissions(Permissions::ADD_ACCOUNT));
        }

        Ok(vec![JournalDomainEvent::AccountCreated {
            account_id: self.account_id,
            journal_id: self.journal_id,
            name: self.name.clone(),
            account_type: self.account_type,
            authority: self.authority,
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

        if !validate_permissions(actor, self.authority, journal.owner, Permissions::OWNER) {
            return Err(JournalError::Permissions(Permissions::OWNER));
        }

        Ok(vec![JournalDomainEvent::AccountRenamed {
            account_id: self.account_id,
            journal_id: self.journal_id,
            new_name: self.name.clone(),
            authority: self.authority,
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

        if !validate_permissions(actor, self.authority, journal.owner, Permissions::OWNER) {
            return Err(JournalError::Permissions(Permissions::OWNER));
        }

        Ok(vec![JournalDomainEvent::AccountDeleted {
            account_id: self.account_id,
            journal_id: self.journal_id,
            authority: self.authority,
            timestamp: self.timestamp,
        }])
    }
}

pub struct AccountState {
    pub id: AccountId,
    #[expect(unused)]
    pub journal_id: JournalId,
    #[expect(unused)]
    pub account_type: AccountType,
    pub name: Name,
    pub balance: i64,
}

#[derive(FromRow)]
struct AccountStateWithPayload {
    id: AccountId,
    #[expect(unused)]
    journal_id: JournalId,
    name: Name,
    account_type: AccountType,
    balance: i64,
    payload: Vec<u8>,
}

impl JournalService {
    pub async fn create_account(
        &self,
        account_id: AccountId,
        journal_id: JournalId,
        name: Name,
        account_type: AccountType,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(CreateAccount::new(
                account_id,
                journal_id,
                name,
                account_type,
                authority,
                timestamp,
            ))
            .await?
            .event_id())
    }

    pub async fn list_journal_accounts(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(AccountState, Authority, Timestamp)>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(JournalError::InvalidJournal(journal_id));
        }

        let accounts = sqlx::query_as!(
            AccountStateWithPayload,
            r#"
            SELECT a.id as "id: AccountId", a.journal_id as "journal_id: JournalId", a.balance, a.name as "name: Name", a.account_type as "account_type: AccountType", e.payload as "payload!"
            FROM accounts a
            INNER JOIN event e
                ON e.account_id = a.id AND e.event_type = 'AccountCreated'
            WHERE a.journal_id = $1
            "#,
            journal_id as JournalId)
            .fetch_all(&self.projection_pool)
            .await?;

        let mut accounts_with_meta = Vec::with_capacity(accounts.len());

        for account in accounts {
            let payload = JournalDomainEvent::try_from(ProtoJournalDomainEvent::decode(
                account.payload.as_slice(),
            )?)?;

            match payload {
                JournalDomainEvent::AccountCreated {
                    authority,
                    timestamp,
                    ..
                } => {
                    accounts_with_meta.push((
                        AccountState {
                            id: account.id,
                            journal_id,
                            name: account.name,
                            balance: account.balance,
                            account_type: account.account_type,
                        },
                        authority,
                        timestamp,
                    ));
                }
                _ => unreachable!("AccountCreated events are filtered by the sql query"),
            }
        }

        Ok(accounts_with_meta)
    }
}
