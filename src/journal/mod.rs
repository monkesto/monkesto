pub mod account;
pub mod activity;
pub mod commands;
pub mod donation;
pub mod error;
pub mod event;
#[expect(unused)]
mod file;
pub mod fund;
pub mod jewel;
pub mod layout;
pub mod member;
pub mod person;
mod projection;
pub mod service;
pub mod transaction;
pub mod views;

use crate::id::Ident;
use crate::journal::event::JournalDomainEvent;
use std::cmp::PartialEq;

use axum::Router;
use axum::routing::{get, post, put};
use axum_login::login_required;

id!(JournalId, Ident::new16());

pub fn router() -> Router<crate::StateType> {
    Router::new()
        .route("/journal", get(views::journal_list))
        .route("/createjournal", post(commands::create_journal))
        .route("/journal/{id}", get(views::journal_detail))
        .route("/journal/{id}/fund", get(fund::views::fund_list_page))
        .route(
            "/journal/{id}/createfund",
            post(fund::commands::create_fund),
        )
        .route(
            "/journal/{id}/activity",
            get(activity::views::activity_list_page),
        )
        .route(
            "/journal/{id}/createactivity",
            post(activity::commands::create_activity),
        )
        .route("/journal/{id}/file", get(file::views::file_list_page))
        .route(
            "/journal/{id}/file/upload",
            post(file::commands::upload_file),
        )
        .route(
            "/journal/{id}/file/localupload",
            put(file::commands::localstore_file_handler)
                .layer(DefaultBodyLimit::max(50 * 1024 * 1024)),
        )
        .route(
            "/journal/{id}/file/recordupload",
            post(file::commands::record_file_upload),
        )
        .route(
            "/journal/{id}/file/{file_id}",
            get(file::views::download_file),
        )
        .route("/journal/{id}/file/{file_id}/jewel", get(jewel::view_db))
        .route(
            "/journal/{id}/file/{file_id}/import_jewel",
            get(jewel::commands::import_jewel_db),
        )
        .route(
            "/journal/{journal_id}/file/{file_id}/delete",
            post(delete_file),
        )
        .route("/journal/{id}/person", get(person::people_list_page))
        .route("/journal/{id}/invite", post(commands::invite_member))
        .route(
            "/journal/{id}/person/{person_id}",
            get(person::person_detail_page),
        )
        .route(
            "/journal/{id}/person/{person_id}/update",
            post(commands::update_permissions),
        )
        .route(
            "/journal/{id}/person/{person_id}/remove",
            post(commands::remove_member),
        )
        .route_layer(login_required!(crate::BackendType, login_url = "/signin"))
}

use crate::authn::UserId;
use crate::authority::Authority;
use crate::id;
use crate::journal::error::JournalError::InvalidJournal;
use crate::journal::error::JournalResult;
use crate::journal::file::commands::delete_file;
use crate::journal::fund::FundId;
use crate::journal::service::stream_lock::{
    append_creation_event_in_tx, append_modification_event_in_tx,
};
use crate::journal::service::{EventId, EventType, JournalService, StreamType};
use crate::name::Name;
use crate::time::Timestamp;
use axum::extract::DefaultBodyLimit;
use bitflags::bitflags;
use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use std::fmt::Display;
use std::fmt::Formatter;
use std::str::FromStr;
use thiserror::Error;

bitflags! {
    #[derive(Hash, Default, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Permissions: i32 {
        const READ = 1 << 0;
        const ADD_ACCOUNT = 1 << 1;
        const CREATE_ACTIVITY = 1 << 2;
        const CREATE_FUND = 1 << 3;
        const UPLOAD_FILE = 1 << 4;
        const APPEND_TRANSACTION = 1 << 5;
        const INVITE = 1 << 6;
        const OWNER = 1 << 7;
    }
}

impl Type<Postgres> for Permissions {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <i32 as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for Permissions {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <i32 as Encode<Postgres>>::encode(self.bits(), buf)
    }
}

impl<'r> Decode<'r, Postgres> for Permissions {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let val = <i32 as Decode<Postgres>>::decode(value)?;
        Ok(Permissions::from_bits(val).ok_or(PermissionDecodeError(val))?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Error, PartialEq)]
pub struct PermissionDecodeError(pub i32);

impl Display for PermissionDecodeError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "an unknown bit was set in the permission value: {}",
            self.0
        )
    }
}

pub struct JournalState {
    pub id: JournalId,
    pub owner_id: UserId,
    pub name: Name,
}

impl JournalService {
    pub async fn create_journal(
        &self,
        journal_id: JournalId,
        owner: UserId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        append_creation_event_in_tx(
            &mut tx,
            JournalDomainEvent::JournalCreated {
                journal_id,
                owner,
                name,
            },
            Some(owner),
            authority,
            timestamp,
            None,
        )
        .await?;

        // seed the general fund upon journal creation
        let event_id = append_creation_event_in_tx(
            &mut tx,
            JournalDomainEvent::FundCreated {
                fund_id: FundId::from_str("gfund")?,
                journal_id,
                fund_name: Name::try_new("General".to_string())?,
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
    pub async fn delete_journal(
        &self,
        journal_id: JournalId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        let stream_handle = service::stream_lock::get_stream_handle(
            &mut tx,
            journal_id,
            *journal_id,
            StreamType::Journal,
        )
        .await?;

        let event_id = append_modification_event_in_tx(
            &mut tx,
            stream_handle,
            JournalDomainEvent::JournalDeleted { journal_id },
            None,
            authority,
            timestamp,
            None,
        )
        .await?;

        tx.commit().await?;
        Ok(event_id)
    }

    pub async fn get_journal(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<(JournalState, Authority, Timestamp)> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(InvalidJournal(journal_id));
        }

        let journal = sqlx::query!(
            r#"
            SELECT j.id as "id: JournalId", j.owner_id as "owner_id: UserId", j.name as "name: Name", e.authority as "authority: Authority", e.timestamp as "timestamp: Timestamp", e.display_timestamp as "display_timestamp: Timestamp"
            FROM journals j
            INNER JOIN journal_events e
                ON e.journal_id = $1 AND e.stream_type = $2 AND e.event_type = $3
            WHERE j.id = $1
            "#,
            journal_id as JournalId,
            StreamType::Journal as i16,
            EventType::JournalCreated as i16
        )
            .fetch_optional(&self.pool)
            .await?;

        if let Some(journal) = journal {
            let timestamp = journal.display_timestamp.unwrap_or(journal.timestamp);

            Ok((
                JournalState {
                    id: journal.id,
                    owner_id: journal.owner_id,
                    name: journal.name,
                },
                journal.authority,
                timestamp,
            ))
        } else {
            Err(InvalidJournal(journal_id))
        }
    }

    pub async fn list_accessible_journals(
        &self,
        user: UserId,
    ) -> JournalResult<Vec<(JournalState, Authority, Timestamp)>> {
        let journals = sqlx::query!(
            r#"
            SELECT j.id as "id: JournalId", j.owner_id as "owner_id: UserId", j.name as "name: Name", e.authority as "authority: Authority", e.timestamp as "timestamp: Timestamp", e.display_timestamp as "display_timestamp: Timestamp"
            FROM journals j
            INNER JOIN journal_events e
                ON e.journal_id = j.id AND e.stream_type = $1 AND e.event_type = $2
            LEFT JOIN journal_members jm ON jm.journal_id = j.id AND (jm.permissions & $3) = $3
            WHERE j.owner_id = $4 OR jm.user_id = $4
            "#,
            StreamType::Journal as i16,
            EventType::JournalCreated as i16,
            Permissions::READ.bits(),
            user as UserId)
            .fetch_all(&self.pool)
            .await?.into_iter().map(
            |j| {
                let timestamp = j.display_timestamp.unwrap_or(j.timestamp);

                (JournalState {
                    id: j.id,
                    owner_id: j.owner_id,
                    name: j.name,
                }, j.authority, timestamp)
            }
        ).collect();

        Ok(journals)
    }
}
