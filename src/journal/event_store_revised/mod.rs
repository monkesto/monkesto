mod projection;

use crate::authn::UserId;
use crate::authority::{Actor, Authority};
use crate::id::Ident;
use crate::journal::JournalError;
use crate::journal::JournalId;
use crate::journal::JournalResult;
use crate::journal::Permissions;
use crate::journal::account::{AccountId, AccountType};
use crate::journal::activity::{ActivityId, ActivityKind};
use crate::journal::entry::{EntryId, EntryKind, EntrySide};
use crate::journal::error::TransactionValidationError;
use crate::journal::event::JournalDomainEvent;
use crate::journal::file::{FileId, ObjectStore};
use crate::journal::fund::{Fund, FundId};
use crate::journal::jewel::JewelImportError;
use crate::journal::jewel::extract::JewelData;
use crate::journal::transaction::memo::Memo;
use crate::journal::transaction::{
    FinancialPeriod, TransactionEntries, TransactionEntryIds, TransactionId,
};
use crate::name::Name;
use crate::time::{TimeProvider, Timestamp};
use chrono::Utc;
use sqlx::migrate::MigrateError;
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::{HashMap, HashSet};
use std::time::Duration;
use thiserror::Error;

pub type EventId = i64;
pub type Version = i64;

#[derive(Clone)]
pub struct JournalService {
    pub pool: PgPool,
    pub object_store: ObjectStore,
}

#[repr(i16)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum StreamType {
    Journal = 1,
    Account = 2,
    Activity = 3,
    Fund = 4,
    Entry = 5,
    Transaction = 6,
    File = 7,
}

#[derive(Error, Debug)]
#[error("invalid stream type: {0}")]
pub struct StreamTypeFromIntError(i16);

impl TryFrom<i16> for StreamType {
    type Error = StreamTypeFromIntError;
    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            x if x == StreamType::Journal as i16 => Ok(StreamType::Journal),
            x if x == StreamType::Account as i16 => Ok(StreamType::Account),
            x if x == StreamType::Activity as i16 => Ok(StreamType::Activity),
            x if x == StreamType::Fund as i16 => Ok(StreamType::Fund),
            x if x == StreamType::Entry as i16 => Ok(StreamType::Entry),
            x if x == StreamType::File as i16 => Ok(StreamType::File),
            _ => Err(StreamTypeFromIntError(value)),
        }
    }
}

#[repr(i16)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum EventType {
    JournalCreated = 101,
    JournalDeleted = 102,
    MemberAdded = 103,
    MemberPermissionsUpdated = 104,
    MemberRemoved = 105,
    AccountCreated = 201,
    AccountRenamed = 202,
    AccountDeleted = 203,
    FundCreated = 301,
    ActivityCreated = 401,
    EntryCreated = 501,
    TransactionCreated = 601,
    FileUploaded = 701,
    FileDeleted = 702,
}

#[derive(Error, Debug)]
#[error("invalid event type: {0}")]
pub struct EventTypeFromIntError(i16);

impl TryFrom<i16> for EventType {
    type Error = EventTypeFromIntError;
    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            x if x == EventType::JournalCreated as i16 => Ok(EventType::JournalCreated),
            x if x == EventType::JournalDeleted as i16 => Ok(EventType::JournalDeleted),
            x if x == EventType::MemberAdded as i16 => Ok(EventType::MemberAdded),
            x if x == EventType::MemberPermissionsUpdated as i16 => {
                Ok(EventType::MemberPermissionsUpdated)
            }
            x if x == EventType::MemberRemoved as i16 => Ok(EventType::MemberRemoved),
            x if x == EventType::AccountCreated as i16 => Ok(EventType::AccountCreated),
            x if x == EventType::AccountRenamed as i16 => Ok(EventType::AccountRenamed),
            x if x == EventType::AccountDeleted as i16 => Ok(EventType::AccountDeleted),
            x if x == EventType::FundCreated as i16 => Ok(EventType::FundCreated),
            x if x == EventType::ActivityCreated as i16 => Ok(EventType::ActivityCreated),
            x if x == EventType::EntryCreated as i16 => Ok(EventType::EntryCreated),
            x if x == EventType::TransactionCreated as i16 => Ok(EventType::TransactionCreated),
            x if x == EventType::FileUploaded as i16 => Ok(EventType::FileUploaded),
            x if x == EventType::FileDeleted as i16 => Ok(EventType::FileDeleted),
            _ => Err(EventTypeFromIntError(value)),
        }
    }
}

fn handle_unique_violation(
    error: sqlx::Error,
    stream_type: StreamType,
    stream_id: Ident,
) -> JournalError {
    if let sqlx::Error::Database(ref db_error) = error
        && db_error.is_unique_violation()
    {
        return match stream_type {
            StreamType::Journal => JournalError::IdCollision(JournalId(stream_id)),
            StreamType::Account => JournalError::AccountIdCollision(AccountId(stream_id)),
            StreamType::Activity => JournalError::ActivityIdCollision(ActivityId(stream_id)),
            StreamType::Fund => JournalError::FundIdCollision(FundId(stream_id)),
            StreamType::Entry => JournalError::EntryIdCollision(EntryId(stream_id)),
            StreamType::Transaction => {
                JournalError::TransactionIdCollision(TransactionId(stream_id))
            }
            StreamType::File => JournalError::FileIdCollision(FileId(stream_id)),
        };
    }
    JournalError::from(error)
}

enum MemberPayload {
    CreatedJournal,
    AddedToJournal(Permissions),
    PermissionsUpdated(Permissions),
    RemovedFromJournal,
}

async fn get_latest_member_payload(
    tx: &mut Transaction<'_, Postgres>,
    journal_id: JournalId,
    user_id: UserId,
) -> JournalResult<Option<MemberPayload>> {
    let payload = sqlx::query_scalar!(
        r#"
        SELECT payload as "payload: JournalDomainEvent" FROM journal_events WHERE journal_id = $1 AND stream_id = $1 AND stream_type = $2 AND event_type = ANY($3) AND user_id = $4
        ORDER BY version DESC
        LIMIT 1
        "#,
        journal_id as JournalId,
        StreamType::Journal as i16,
        &[EventType::JournalCreated as i16, EventType::MemberAdded as i16, EventType::MemberPermissionsUpdated as i16, EventType::MemberRemoved as i16],
        user_id as UserId,
    ).fetch_optional(&mut **tx).await?;

    Ok(payload.map(|payload| match payload {
        JournalDomainEvent::JournalCreated { .. } => MemberPayload::CreatedJournal,
        JournalDomainEvent::MemberAdded { permissions, .. } => {
            MemberPayload::AddedToJournal(permissions)
        }
        JournalDomainEvent::MemberPermissionsUpdated { permissions, .. } => {
            MemberPayload::PermissionsUpdated(permissions)
        }
        JournalDomainEvent::MemberRemoved { .. } => MemberPayload::RemovedFromJournal,
        _ => unreachable!("Event types are filtered by the query"),
    }))
}

impl JournalService {
    pub async fn try_new(pool: PgPool) -> Result<Self, MigrateError> {
        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self {
            pool,
            object_store: ObjectStore::new().await,
        })
    }

    /// push the event, and notify the listener
    ///
    /// the caller is responsible for updating the stream
    #[allow(clippy::too_many_arguments)]
    async fn append_event_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        journal_id: JournalId,
        stream_id: Ident,
        version: Version,
        payload: JournalDomainEvent,
        user_id: Option<UserId>,
        authority: Authority,
        timestamp: Timestamp,
        display_timestamp: Option<Timestamp>,
    ) -> JournalResult<EventId> {
        let stream_type = payload.stream_type();
        let event_type = payload.event_type();

        let event_id = sqlx::query_scalar!(
            r#"
            INSERT INTO journal_events (journal_id, stream_type, stream_id, version, event_type, payload, user_id, authority, timestamp, display_timestamp) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING event_id
            "#,
            journal_id as JournalId,
            stream_type as i16,
            stream_id as Ident,
            version,
            event_type as i16,
            payload as JournalDomainEvent,
            user_id as Option<UserId>,
            authority as Authority,
            timestamp as Timestamp,
            display_timestamp as Option<Timestamp>
        ).fetch_one(&mut **tx)
            .await
            .map_err(|e| handle_unique_violation(e, stream_type, stream_id))?;

        sqlx::query!(
            r#"
            SELECT pg_notify('journal_events', $1)
            "#,
            event_id.to_string()
        )
        .execute(&mut **tx)
        .await?;

        Ok(event_id)
    }

    /// checks permissions and acquires a read lock on the journal stream to ensure they will remain valid
    ///
    /// returns Ok(()) if the user has at least the expected permissions
    ///
    /// returns Err(JournalError(Permissions)) if the user doesn't have the expected permissions
    ///
    /// returns Err(JournalError(InvalidJournal)) if the user is not a member of the journal
    ///
    /// returns Err(JournalError(InvalidJournal)) if the journal has been deleted
    async fn check_permissions_in_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        journal_id: JournalId,
        authority: Authority,
        expected_permissions: Permissions,
    ) -> JournalResult<()> {
        // get a read lock to prevent anyone from appending to the journal stream until the transaction is finished
        let journal_valid = sqlx::query_scalar!(
            r#"
            SELECT valid from journal_stream WHERE journal_id = $1
            FOR SHARE
            "#,
            journal_id as JournalId,
        )
        .fetch_one(&mut **tx)
        .await?;

        if !journal_valid {
            return Err(JournalError::InvalidJournal(journal_id));
        }

        match authority.actor() {
            Actor::User(user_id) => {
                let payload = get_latest_member_payload(tx, journal_id, user_id)
                    .await?
                    .ok_or(JournalError::InvalidJournal(journal_id))?;
                match payload {
                    MemberPayload::CreatedJournal => Ok(()),
                    MemberPayload::AddedToJournal(permissions)
                    | MemberPayload::PermissionsUpdated(permissions) => {
                        if permissions.contains(expected_permissions) {
                            Ok(())
                        } else {
                            Err(JournalError::Permissions(expected_permissions))
                        }
                    }
                    MemberPayload::RemovedFromJournal => {
                        Err(JournalError::InvalidJournal(journal_id))
                    }
                }
            }
            Actor::System => Ok(()),
            Actor::Anonymous => Err(JournalError::Permissions(expected_permissions)),
        }
    }

    async fn create_journal(
        &self,
        journal_id: JournalId,
        name: Name,
        owner: UserId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        // no locks are required because there isn't any validation needed
        // journal id conflicts will surface as sql unique violations

        let event_id = self
            .append_event_in_tx(
                &mut tx,
                journal_id,
                *journal_id,
                0,
                JournalDomainEvent::JournalCreated {
                    journal_id,
                    owner,
                    name,
                    authority,
                    timestamp,
                },
                Some(owner),
                authority,
                timestamp,
                None,
            )
            .await?;

        sqlx::query!(
            r#"
            INSERT INTO journal_stream (journal_id, version, valid) VALUES ($1, 0, true)
            "#,
            journal_id as JournalId,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(event_id)
    }

    async fn delete_journal(
        &self,
        journal_id: JournalId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        // get a write lock on the journal
        let stream_query = sqlx::query!(
            r#"
            SELECT version, valid FROM journal_stream
            WHERE journal_id = $1
            FOR UPDATE
            "#,
            journal_id as JournalId,
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(query) = stream_query {
            if !query.valid {
                return Err(JournalError::InvalidJournal(journal_id));
            }

            self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::OWNER)
                .await?;

            let event_id = self
                .append_event_in_tx(
                    &mut tx,
                    journal_id,
                    *journal_id,
                    query.version + 1,
                    JournalDomainEvent::JournalDeleted {
                        journal_id,
                        authority,
                        timestamp,
                    },
                    None,
                    authority,
                    timestamp,
                    None,
                )
                .await?;

            sqlx::query!(
                r#"
                UPDATE journal_stream SET version = $1, valid = false WHERE journal_id = $2
                "#,
                query.version + 1,
                journal_id as JournalId,
            )
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok(event_id)
        } else {
            Err(JournalError::InvalidJournal(journal_id))
        }
    }

    async fn add_journal_member(
        &self,
        journal_id: JournalId,
        user_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<i64> {
        let mut tx = self.pool.begin().await?;

        // get a write lock on the journal
        let stream_query = sqlx::query!(
            r#"
            SELECT version, valid FROM journal_stream
            WHERE journal_id = $1
            FOR UPDATE
            "#,
            journal_id as JournalId,
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(query) = stream_query {
            if !query.valid {
                return Err(JournalError::InvalidJournal(journal_id));
            }

            self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::INVITE)
                .await?;

            let member_payload = get_latest_member_payload(&mut tx, journal_id, user_id).await?;

            if let Some(payload) = member_payload {
                match payload {
                    MemberPayload::AddedToJournal(..)
                    | MemberPayload::PermissionsUpdated(..)
                    | MemberPayload::CreatedJournal => {
                        return Err(JournalError::UserAlreadyHasAccess(user_id));
                    }

                    MemberPayload::RemovedFromJournal => {
                        // allow re-invitation of removed users
                    }
                }
            }

            let event_id = self
                .append_event_in_tx(
                    &mut tx,
                    journal_id,
                    *journal_id,
                    query.version + 1,
                    JournalDomainEvent::MemberAdded {
                        journal_id,
                        user_id,
                        permissions,
                        authority,
                        timestamp,
                    },
                    Some(user_id),
                    authority,
                    timestamp,
                    None,
                )
                .await?;

            sqlx::query!(
                r#"
                UPDATE journal_stream SET version = $1 WHERE journal_id = $2
                "#,
                query.version + 1,
                journal_id as JournalId,
            )
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok(event_id)
        } else {
            Err(JournalError::InvalidJournal(journal_id))
        }
    }

    async fn change_member_permissions(
        &self,
        journal_id: JournalId,
        user_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<i64> {
        let mut tx = self.pool.begin().await?;

        // get a write lock on the journal
        let stream_query = sqlx::query!(
            r#"
            SELECT version, valid FROM journal_stream
            WHERE journal_id = $1
            FOR UPDATE
            "#,
            journal_id as JournalId,
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(query) = stream_query {
            if !query.valid {
                return Err(JournalError::InvalidJournal(journal_id));
            }

            self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::OWNER)
                .await?;

            let latest_member_payload = get_latest_member_payload(&mut tx, journal_id, user_id)
                .await?
                .ok_or(JournalError::UserDoesntHaveAccess(user_id))?;
            match latest_member_payload {
                MemberPayload::AddedToJournal(..) | MemberPayload::PermissionsUpdated(..) => {}

                MemberPayload::CreatedJournal => {
                    return Err(JournalError::OwnerModification(user_id));
                }

                MemberPayload::RemovedFromJournal => {
                    return Err(JournalError::UserDoesntHaveAccess(user_id));
                }
            }

            let event_id = self
                .append_event_in_tx(
                    &mut tx,
                    journal_id,
                    *journal_id,
                    query.version + 1,
                    JournalDomainEvent::MemberPermissionsUpdated {
                        journal_id,
                        user_id,
                        permissions,
                        authority,
                        timestamp,
                    },
                    Some(user_id),
                    authority,
                    timestamp,
                    None,
                )
                .await?;

            sqlx::query!(
                r#"
            UPDATE journal_stream SET version = $1 WHERE journal_id = $2
            "#,
                query.version + 1,
                journal_id as JournalId,
            )
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok(event_id)
        } else {
            Err(JournalError::UserDoesntHaveAccess(user_id))
        }
    }

    async fn remove_journal_member(
        &self,
        journal_id: JournalId,
        user_id: UserId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<i64> {
        let mut tx = self.pool.begin().await?;

        // get a write lock on the journal
        let stream_query = sqlx::query!(
            r#"
            SELECT version, valid FROM journal_stream
            WHERE journal_id = $1
            FOR UPDATE
            "#,
            journal_id as JournalId,
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(query) = stream_query {
            if !query.valid {
                return Err(JournalError::InvalidJournal(journal_id));
            }

            self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::OWNER)
                .await?;

            let latest_member_payload = get_latest_member_payload(&mut tx, journal_id, user_id)
                .await?
                .ok_or(JournalError::UserDoesntHaveAccess(user_id))?;

            match latest_member_payload {
                MemberPayload::CreatedJournal => {
                    return Err(JournalError::OwnerModification(user_id));
                }
                MemberPayload::AddedToJournal(..) | MemberPayload::PermissionsUpdated(..) => {}

                MemberPayload::RemovedFromJournal => {
                    return Err(JournalError::UserDoesntHaveAccess(user_id));
                }
            }

            let event_id = self
                .append_event_in_tx(
                    &mut tx,
                    journal_id,
                    *journal_id,
                    query.version + 1,
                    JournalDomainEvent::MemberRemoved {
                        journal_id,
                        user_id,
                        authority,
                        timestamp,
                    },
                    Some(user_id),
                    authority,
                    timestamp,
                    None,
                )
                .await?;

            sqlx::query!(
                r#"
                UPDATE journal_stream SET version = $1 WHERE journal_id = $2
                "#,
                query.version + 1,
                journal_id as JournalId,
            )
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok(event_id)
        } else {
            Err(JournalError::InvalidJournal(journal_id))
        }
    }

    async fn create_account(
        &self,
        journal_id: JournalId,
        account_id: AccountId,
        account_type: AccountType,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::ADD_ACCOUNT)
            .await?;

        let event_id = self
            .append_event_in_tx(
                &mut tx,
                journal_id,
                *account_id,
                0,
                JournalDomainEvent::AccountCreated {
                    journal_id,
                    account_id,
                    account_type,
                    name,
                    authority,
                    timestamp,
                },
                None,
                authority,
                timestamp,
                None,
            )
            .await?;

        sqlx::query!(
            r#"
            INSERT INTO account_stream (journal_id, account_id, version, valid) VALUES ($1, $2, 0, true)
            "#,
            journal_id as JournalId,
            account_id as AccountId,
        )
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(event_id)
    }

    async fn rename_account(
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

        // get a write lock on the account
        let stream_query = sqlx::query!(
            r#"
            SELECT version, valid FROM account_stream WHERE journal_id = $1 AND account_id = $2
            FOR UPDATE
            "#,
            journal_id as JournalId,
            account_id as AccountId,
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(query) = stream_query {
            if !query.valid {
                return Err(JournalError::InvalidAccount(account_id));
            }

            let event_id = self
                .append_event_in_tx(
                    &mut tx,
                    journal_id,
                    *account_id,
                    query.version + 1,
                    JournalDomainEvent::AccountRenamed {
                        journal_id,
                        account_id,
                        new_name,
                        authority,
                        timestamp,
                    },
                    None,
                    authority,
                    timestamp,
                    None,
                )
                .await?;

            sqlx::query!(
                r#"
                UPDATE account_stream SET version = $1 WHERE journal_id = $2 AND account_id = $3
                "#,
                query.version + 1,
                journal_id as JournalId,
                account_id as AccountId,
            )
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok(event_id)
        } else {
            Err(JournalError::InvalidAccount(account_id))
        }
    }

    async fn delete_account(
        &self,
        journal_id: JournalId,
        account_id: AccountId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::OWNER)
            .await?;

        // get a write lock on the account
        let stream_query = sqlx::query!(
            r#"
            SELECT version, valid FROM account_stream WHERE journal_id = $1 AND account_id = $2
            FOR UPDATE
            "#,
            journal_id as JournalId,
            account_id as AccountId,
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(query) = stream_query {
            if !query.valid {
                return Err(JournalError::InvalidAccount(account_id));
            }

            let event_id = self
                .append_event_in_tx(
                    &mut tx,
                    journal_id,
                    *account_id,
                    query.version + 1,
                    JournalDomainEvent::AccountDeleted {
                        journal_id,
                        account_id,
                        authority,
                        timestamp,
                    },
                    None,
                    authority,
                    timestamp,
                    None,
                )
                .await?;

            sqlx::query!(
                r#"
                UPDATE account_stream SET version = $1, valid = false WHERE journal_id = $2 AND account_id = $3
                "#,
                query.version + 1,
                journal_id as JournalId,
                account_id as AccountId,
            )
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok(event_id)
        } else {
            Err(JournalError::InvalidAccount(account_id))
        }
    }

    async fn create_activity(
        &self,
        journal_id: JournalId,
        activity_id: ActivityId,
        activity_kind: ActivityKind,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::CREATE_ACTIVITY)
            .await?;

        let event_id = self
            .append_event_in_tx(
                &mut tx,
                journal_id,
                *activity_id,
                0,
                JournalDomainEvent::ActivityCreated {
                    journal_id,
                    activity_id,
                    activity_name: name,
                    activity_kind,
                    authority,
                    timestamp,
                },
                None,
                authority,
                timestamp,
                None,
            )
            .await?;

        sqlx::query!(
            r#"
            INSERT INTO activity_stream (journal_id, activity_id, version, valid) VALUES ($1, $2, 0, true)
            "#,
            journal_id as JournalId,
            activity_id as ActivityId,
        )
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(event_id)
    }

    async fn create_fund(
        &self,
        journal_id: JournalId,
        fund_id: FundId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::CREATE_FUND)
            .await?;

        let event_id = self
            .append_event_in_tx(
                &mut tx,
                journal_id,
                *fund_id,
                0,
                JournalDomainEvent::FundCreated {
                    journal_id,
                    fund_id,
                    fund_name: name,
                    authority,
                    timestamp,
                },
                None,
                authority,
                timestamp,
                None,
            )
            .await?;

        sqlx::query!(
            r#"
            INSERT INTO fund_stream (journal_id, fund_id, version, valid) VALUES ($1, $2, 0, true)
            "#,
            journal_id as JournalId,
            fund_id as FundId,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(event_id)
    }

    async fn upload_file(
        &self,
        journal_id: JournalId,
        file_id: FileId,
        file_name: String,
        file_hash: [u8; 16],
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::UPLOAD_FILE)
            .await?;

        let event_id = self
            .append_event_in_tx(
                &mut tx,
                journal_id,
                *file_id,
                0,
                JournalDomainEvent::FileUploaded {
                    file_id,
                    journal_id,
                    hash: file_hash,
                    file_name,
                    authority,
                    timestamp,
                },
                None,
                authority,
                timestamp,
                None,
            )
            .await?;

        sqlx::query!(
            r#"
            INSERT INTO file_stream (journal_id, file_id, version, valid) VALUES ($1, $2, 0, true)
            "#,
            journal_id as JournalId,
            file_id as FileId
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(event_id)
    }

    async fn delete_file(
        &self,
        journal_id: JournalId,
        file_id: FileId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::OWNER)
            .await?;

        let stream_query = sqlx::query!(
            r#"
            SELECT version, valid FROM file_stream WHERE journal_id = $1 AND file_id = $2
            FOR UPDATE
            "#,
            journal_id as JournalId,
            file_id as FileId,
        )
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(query) = stream_query {
            if !query.valid {
                return Err(JournalError::InvalidFile(file_id));
            }

            let event_id = self
                .append_event_in_tx(
                    &mut tx,
                    journal_id,
                    *file_id,
                    query.version + 1,
                    JournalDomainEvent::FileDeleted {
                        file_id,
                        journal_id,
                        authority,
                        timestamp,
                    },
                    None,
                    authority,
                    timestamp,
                    None,
                )
                .await?;

            sqlx::query!(
                r#"
            UPDATE file_stream SET version = $1, valid = false WHERE journal_id = $2 AND file_id = $3
            "#,
                query.version + 1,
                journal_id as JournalId,
                file_id as FileId,
            )
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;
            Ok(event_id)
        } else {
            Err(JournalError::InvalidFile(file_id))
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn create_transaction(
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
        let mut entry_ids = Vec::new();

        for entry in entries.iter() {
            match entry.entry_kind {
                EntryKind::Account { account_id } => {
                    account_ids.push(account_id);
                }
                EntryKind::Activity {
                    activity_id,
                    fund_id,
                    scope: _,
                } => {
                    activity_ids.push(activity_id);
                    fund_ids.push(fund_id);
                }
            }
        }

        // read lock all relevant accounts, activities, and funds
        let accounts: HashMap<AccountId, bool> = sqlx::query!(
            r#"
            SELECT account_id as "account_id: AccountId", valid FROM account_stream WHERE journal_id = $1 AND account_id = ANY($2)
            ORDER BY account_id
            FOR SHARE
            "#,
            journal_id as JournalId,
            &account_ids as &Vec<AccountId>,
        )
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|r| (r.account_id, r.valid)).collect();

        let activities: HashMap<ActivityId, bool> = sqlx::query!(
            r#"
            SELECT activity_id as "activity_id: ActivityId", valid FROM activity_stream WHERE journal_id = $1 AND activity_id = ANY($2)
            ORDER BY activity_id
            FOR SHARE
            "#,
            journal_id as JournalId,
            &activity_ids as &Vec<ActivityId>,
        )
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|r| (r.activity_id, r.valid)).collect();

        let funds: HashMap<FundId, bool> = sqlx::query!(
            r#"
            SELECT fund_id as "fund_id: FundId", valid FROM fund_stream WHERE journal_id = $1 AND fund_id = ANY($2)
            ORDER BY fund_id
            FOR SHARE
            "#,
            journal_id as JournalId,
            &fund_ids as &Vec<FundId>,
        )
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|r| (r.fund_id, r.valid)).collect();

        let mut overall_balance = 0;

        for entry in entries.iter() {
            let entry_change = match entry.entry_side {
                EntrySide::Credit => entry.amount as i64,
                EntrySide::Debit => -(entry.amount as i64),
            };

            overall_balance += entry_change;

            match entry.entry_kind {
                EntryKind::Account { account_id } => {
                    if !accounts.get(&account_id).copied().unwrap_or(false) {
                        return Err(JournalError::InvalidAccount(account_id));
                    }
                }
                EntryKind::Activity {
                    activity_id,
                    fund_id,
                    scope: _,
                } => {
                    // TODO(Gabriel): Where does scope fit in?
                    if !activities.get(&activity_id).copied().unwrap_or(false) {
                        return Err(JournalError::InvalidActivity(activity_id));
                    }

                    if !funds.get(&fund_id).copied().unwrap_or(false) {
                        return Err(JournalError::InvalidFund(fund_id));
                    }
                }
            }

            let entry_id = EntryId::new();

            entry_ids.push(entry_id);

            self.append_event_in_tx(
                &mut tx,
                journal_id,
                *entry_id,
                0,
                JournalDomainEvent::EntryCreated {
                    entry_id,
                    journal_id,
                    amount: entry.amount,
                    entry_side: entry.entry_side,
                    entry_kind: entry.entry_kind,
                    authority,
                    timestamp,
                },
                None,
                authority,
                timestamp,
                None,
            )
            .await?;

            sqlx::query!(
            r#"
            INSERT INTO entry_stream (journal_id, transaction_id, entry_id, version, valid) VALUES ($1, $2, $3, 0, true)
            "#,
            journal_id as JournalId,
            transaction_id as TransactionId,
            entry_id as EntryId,
        )
            .execute(&mut *tx)
            .await?;
        }

        if overall_balance != 0 {
            return Err(JournalError::TransactionValidation(
                TransactionValidationError::ImbalancedTransaction(entries.clone()),
            ));
        }

        let event_id = self
            .append_event_in_tx(
                &mut tx,
                journal_id,
                *transaction_id,
                0,
                JournalDomainEvent::TransactionCreated {
                    transaction_id,
                    journal_id,
                    // TODO(Gabriel): we can query associated entries through the entry stream, this is only here for disintegrate
                    entries: TransactionEntryIds(entry_ids),
                    financial_period: period,
                    memo,
                    authority,
                    timestamp,
                },
                None,
                authority,
                timestamp,
                None,
            )
            .await?;

        sqlx::query!(
            r#"
            INSERT INTO transaction_stream (journal_id, transaction_id, version, valid) VALUES ($1, $2, 0, true)
            "#,
            journal_id as JournalId,
            transaction_id as TransactionId,
        )
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        Ok(event_id)
    }

    #[allow(clippy::too_many_arguments)]
    async fn import_jewel_database(
        &self,
        journal_id: JournalId,
        jewel_data: JewelData,
        authority: Authority,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;
        let mut event_id = 0;

        self.check_permissions_in_tx(
            &mut tx,
            journal_id,
            authority,
            Permissions::ADD_ACCOUNT
                | Permissions::CREATE_ACTIVITY
                | Permissions::CREATE_FUND
                | Permissions::APPEND_TRANSACTION,
        )
        .await?;

        let mut account_creation_timestamp = Utc::now();
        let mut account_id_map = HashMap::new();

        for (jewel_account_id, account) in jewel_data.accounts {
            if account.active {
                let account_type = if account.account_type < 3 {
                    AccountType::Asset
                } else {
                    AccountType::Liability
                };

                let account_id = AccountId::new();
                account_id_map.insert(jewel_account_id, account_id);

                // jewel doesn't record account creation timestamps, but its ids are sequential
                // since we're creating the accounts in that sequential order, we can keep the order
                // by creating each account 1 millisecond later than the last
                account_creation_timestamp += Duration::from_millis(1);

                event_id = self
                    .append_event_in_tx(
                        &mut tx,
                        journal_id,
                        *journal_id,
                        0,
                        JournalDomainEvent::AccountCreated {
                            journal_id,
                            account_id,
                            account_type,
                            name: Name::try_new(account.name.clone())?,
                            authority,
                            timestamp: account_creation_timestamp.get_time(),
                        },
                        None,
                        authority,
                        account_creation_timestamp.get_time(),
                        None,
                    )
                    .await?;

                sqlx::query!(
            r#"
            INSERT INTO account_stream (journal_id, account_id, version, valid) VALUES ($1, $2, 0, true)
            "#,
            journal_id as JournalId,
            account_id as AccountId,
        )
        .execute(&mut *tx)
        .await?;
            }
        }

        for (jewel_transaction_id, jewel_transaction) in jewel_data.journals {
            let transaction_id = TransactionId::new();
            let z_single_account_id = jewel_transaction.z_single_account_id;

            let mut entry_ids = Vec::new();
            let mut entry_sum = 0;

            for entry in jewel_data
                .journal_entries
                .get(&jewel_transaction_id)
                .ok_or(JewelImportError::MissingJournalEntries(
                    jewel_transaction_id,
                ))?
            {
                if entry.account_id == jewel_transaction.z_single_account_id {
                    continue;
                }

                let entry_amount = (entry.amount * 100.0).round() as i64;

                entry_sum += entry_amount;

                let entry_id = EntryId::new();
                entry_ids.push(entry_id);

                self.append_event_in_tx(
                    &mut tx,
                    journal_id,
                    *entry_id,
                    0,
                    JournalDomainEvent::EntryCreated {
                        entry_id,
                        journal_id,
                        amount: entry_amount as u64,
                        entry_side: if entry_amount > 0 {
                            EntrySide::Credit
                        } else {
                            EntrySide::Debit
                        },
                        entry_kind: EntryKind::Account {
                            account_id: *account_id_map
                                .get(&entry.account_id)
                                .ok_or(JewelImportError::MissingAccount(entry.account_id))?,
                        },
                        authority,
                        timestamp: jewel_transaction.accounting_date.get_time(),
                    },
                    None,
                    authority,
                    jewel_transaction.accounting_date.get_time(),
                    None,
                )
                .await?;

                sqlx::query!(
            r#"
            INSERT INTO entry_stream (journal_id, transaction_id, entry_id, version, valid) VALUES ($1, $2, $3, 0, true)
            "#,
            journal_id as JournalId,
            transaction_id as TransactionId,
            entry_id as EntryId,
            )
            .execute(&mut *tx)
            .await?;
            }

            let z_single_entry_id = EntryId::new();
            entry_ids.push(z_single_entry_id);

            self.append_event_in_tx(
                &mut tx,
                journal_id,
                *z_single_entry_id,
                0,
                JournalDomainEvent::EntryCreated {
                    entry_id: z_single_entry_id,
                    journal_id,
                    amount: entry_sum.unsigned_abs(),
                    entry_side: if entry_sum > 0 {
                        EntrySide::Debit
                    } else {
                        EntrySide::Credit
                    },
                    entry_kind: EntryKind::Account {
                        account_id: *account_id_map
                            .get(&z_single_account_id)
                            .ok_or(JewelImportError::MissingAccount(z_single_account_id))?,
                    },
                    authority,
                    timestamp: jewel_transaction.accounting_date.get_time(),
                },
                None,
                authority,
                jewel_transaction.accounting_date.get_time(),
                None,
            )
            .await?;

            sqlx::query!(
            r#"
            INSERT INTO entry_stream (journal_id, transaction_id, entry_id, version, valid) VALUES ($1, $2, $3, 0, true)
            "#,
            journal_id as JournalId,
            transaction_id as TransactionId,
            z_single_entry_id as EntryId,
            )
                .execute(&mut *tx)
                .await?;

            event_id = self
                .append_event_in_tx(
                    &mut tx,
                    journal_id,
                    *transaction_id,
                    0,
                    JournalDomainEvent::TransactionCreated {
                        transaction_id,
                        journal_id,
                        entries: TransactionEntryIds(entry_ids),
                        financial_period: FinancialPeriod::from(jewel_transaction.accounting_date),
                        memo: Some(Memo::try_new(jewel_transaction.memo)?),
                        authority,
                        timestamp: jewel_transaction.accounting_date.get_time(),
                    },
                    None,
                    authority,
                    jewel_transaction.accounting_date.get_time(),
                    None,
                )
                .await?;

            sqlx::query!(
            r#"
            INSERT INTO transaction_stream (journal_id, transaction_id, version, valid) VALUES ($1, $2, 0, true)
            "#,
            journal_id as JournalId,
            transaction_id as TransactionId,
        )
        .execute(&mut *tx)
        .await?;
        }

        tx.commit().await?;

        Ok(event_id)
    }
}
