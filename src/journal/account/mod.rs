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
use crate::journal::error::JournalError::InvalidJournal;
use crate::journal::error::JournalResult;
use crate::journal::event::JournalDomainEvent;
use crate::journal::service::stream_lock::{
    append_creation_event_in_tx, append_modification_event_in_tx, get_stream_handle,
};
use crate::journal::service::{EventId, EventType, JournalService, StreamType};
use crate::journal::{JournalId, Permissions};
use crate::name::Name;
use crate::time::Timestamp;
use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
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

impl JournalService {
    pub async fn create_account(
        &self,
        account_id: AccountId,
        journal_id: JournalId,
        name: Name,
        account_type: AccountType,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::ADD_ACCOUNT)
            .await?;

        let event_id = append_creation_event_in_tx(
            &mut tx,
            JournalDomainEvent::AccountCreated {
                journal_id,
                account_id,
                account_type,
                name,
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

    #[expect(unused)]
    pub async fn rename_account(
        &self,
        journal_id: JournalId,
        account_id: AccountId,
        new_name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::ADD_ACCOUNT)
            .await?;

        let stream_handle =
            get_stream_handle(&mut tx, journal_id, *account_id, StreamType::Account).await?;

        let event_id = append_modification_event_in_tx(
            &mut tx,
            stream_handle,
            JournalDomainEvent::AccountRenamed {
                journal_id,
                account_id,
                new_name,
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

    #[expect(unused)]
    pub async fn delete_account(
        &self,
        journal_id: JournalId,
        account_id: AccountId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::OWNER)
            .await?;

        let stream_handle =
            get_stream_handle(&mut tx, journal_id, *account_id, StreamType::Account).await?;

        let event_id = append_modification_event_in_tx(
            &mut tx,
            stream_handle,
            JournalDomainEvent::AccountDeleted {
                journal_id,
                account_id,
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
            return Err(InvalidJournal(journal_id));
        }

        let accounts = sqlx::query!(
            r#"
            SELECT a.id as "id: AccountId", a.journal_id as "journal_id: JournalId", a.balance, a.name as "name: Name", a.account_type as "account_type: AccountType", e.authority as "authority: Authority", e.timestamp as "timestamp: Timestamp", e.display_timestamp as "display_timestamp: Timestamp"
            FROM accounts a
            INNER JOIN journal_events e
                ON a.journal_id = $1 AND e.stream_id = a.id AND e.stream_type = $2 AND e.event_type = $3
            WHERE a.journal_id = $1
            ORDER BY COALESCE(e.display_timestamp, e.timestamp) DESC
            "#,
            journal_id as JournalId,
            StreamType::Account as i16,
            EventType::AccountCreated as i16,
        )
            .fetch_all(self.pool())
            .await?.into_iter().map(|a| {
            let timestamp = a.display_timestamp.unwrap_or(a.timestamp);
            (AccountState {
                id: a.id,
                journal_id,
                account_type: a.account_type,
                name: a.name,
                balance: a.balance,
            }, a.authority, timestamp)
        }).collect();

        Ok(accounts)
    }
}

pub struct AccountState {
    pub id: AccountId,
    #[expect(unused)]
    pub journal_id: JournalId,
    pub account_type: AccountType,
    pub name: Name,
    pub balance: i64,
}
