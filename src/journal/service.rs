use crate::authn::UserId;
use crate::authority::{Actor, Authority};
use crate::id::Ident;
use crate::journal::account::{AccountId, AccountType};
use crate::journal::activity::ActivityId;
use crate::journal::donation::CheckNum;
use crate::journal::donation::DonationId;
use crate::journal::donation::donation_entry::DonationEntryId;
use crate::journal::donation::donation_entry::DonationInstrument;
use crate::journal::donation::donor::DonorId;
use crate::journal::error::JournalError::InvalidJournal;
use crate::journal::error::{JournalError, JournalResult};
use crate::journal::event::JournalDomainEvent;
use crate::journal::file::{FileId, ObjectStore};
use crate::journal::fund::FundId;
use crate::journal::projection::JournalProjection;
use crate::journal::transaction::entry::{EntryId, EntryKind, EntrySide};
use crate::journal::transaction::memo::Memo;
use crate::journal::transaction::{FinancialPeriod, TransactionId};
use crate::journal::{JournalId, Permissions};
use crate::name::Name;
use axum_login::tracing::error;
use futures_util::TryFutureExt;
use sqlx::{PgPool, PgTransaction, Postgres, Transaction};
use thiserror::Error;

pub type EventId = i64;
pub type Version = i64;

// the methods for getting and using a StreamHandle are placed in a
// submodule to prevent accidental fabrication
pub(crate) mod stream_lock {
    use crate::authn::UserId;
    use crate::authority::Authority;
    use crate::id::Ident;
    use crate::journal::JournalId;
    use crate::journal::error::JournalResult;
    use crate::journal::event::JournalDomainEvent;
    use crate::journal::service::{
        EventAction, EventId, StreamType, Version, handle_missing_stream, handle_unique_violation,
    };
    use crate::time::Timestamp;
    use sqlx::{Postgres, QueryBuilder, Transaction};
    use std::collections::HashMap;

    pub struct StreamHandle {
        journal_id: JournalId,
        stream_id: Ident,
        stream_type: StreamType,
        version: Version,
    }

    #[derive(sqlx::FromRow)]
    struct StreamInfo {
        version: Version,
        valid: bool,
    }

    async fn get_stream_info(
        tx: &mut Transaction<'_, Postgres>,
        journal_id: JournalId,
        stream_id: Ident,
        stream_type: StreamType,
        write_lock: bool,
    ) -> JournalResult<Version> {
        let stream_prefix = stream_type.stream_prefix();

        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT version, valid FROM ");

        builder.push(stream_prefix);
        builder.push("_stream ");

        builder.push("where journal_id = ");
        builder.push_bind(journal_id);

        if !matches!(stream_type, StreamType::Journal) {
            builder.push(" AND ");
            builder.push(stream_prefix);
            builder.push("_id =  ");
            builder.push_bind(stream_id);
        }

        if write_lock {
            builder.push(" FOR UPDATE");
        } else {
            builder.push(" FOR SHARE");
        }

        let stream_info = builder
            .build_query_as::<StreamInfo>()
            .fetch_optional(&mut **tx)
            .await?;

        if let Some(info) = stream_info
            && info.valid
        {
            Ok(info.version)
        } else {
            Err(handle_missing_stream(stream_id, stream_type))
        }
    }

    /// gets a handle on a stream if it exists
    /// returns an error if it doesn't exist or has been deleted
    ///
    /// this method gets a write lock; prefer assert_stream_exists if you do not plan to modify the stream
    pub async fn get_stream_handle(
        tx: &mut Transaction<'_, Postgres>,
        journal_id: JournalId,
        stream_id: Ident,
        stream_type: StreamType,
    ) -> JournalResult<StreamHandle> {
        let version = get_stream_info(tx, journal_id, stream_id, stream_type, true).await?;

        Ok(StreamHandle {
            journal_id,
            stream_id,
            stream_type,
            version,
        })
    }

    /// ensures that a stream exists
    ///
    /// returns an error if it does not exist or has been deleted
    ///
    /// this method gets a read lock on the stream if it exists,
    /// ensuring it won't be modified before the transaction is committed
    #[expect(unused)]
    pub async fn assert_stream_exists(
        tx: &mut Transaction<'_, Postgres>,
        journal_id: JournalId,
        stream_id: Ident,
        stream_type: StreamType,
    ) -> JournalResult<()> {
        _ = get_stream_info(tx, journal_id, stream_id, stream_type, false).await?;

        Ok(())
    }

    /// ensures that multiple streams of the same type exists
    ///
    /// does not support journal streams
    ///
    /// returns an error if any element doesn't exist or has been deleted
    ///
    /// this method gets a read lock on the streams if they exist,
    /// ensuring they won't be modified before the transaction is committed
    pub async fn assert_multiple_streams_exist(
        tx: &mut Transaction<'_, Postgres>,
        journal_id: JournalId,
        stream_ids: &[Ident],
        stream_type: StreamType,
    ) -> JournalResult<()> {
        assert_ne!(
            stream_type,
            StreamType::Journal,
            "unable to assert that multiple journal streams exist"
        );

        let stream_prefix = stream_type.stream_prefix();

        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new("SELECT valid, ");
        builder.push(stream_prefix);
        builder.push("_id FROM ");

        builder.push(stream_prefix);
        builder.push("_stream ");

        builder.push("where journal_id = ");
        builder.push_bind(journal_id);

        builder.push(" AND ");
        builder.push(stream_prefix);
        builder.push("_id =  ANY(");
        builder.push_bind(stream_ids);
        builder.push(")");

        builder.push(" FOR SHARE");

        let stream_info: HashMap<Ident, bool> = builder
            .build_query_as::<(bool, Ident)>()
            .fetch_all(&mut **tx)
            .await?
            .into_iter()
            .map(|(valid, id)| (id, valid))
            .collect();

        for id in stream_ids {
            if !stream_info.get(id).copied().unwrap_or(false) {
                return Err(handle_missing_stream(*id, stream_type));
            }
        }

        Ok(())
    }

    /// push the event, and notify the listener, and update the stream
    #[allow(clippy::too_many_arguments)]
    pub async fn append_creation_event_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        payload: JournalDomainEvent,
        user_id: Option<UserId>,
        authority: Authority,
        timestamp: Timestamp,
        display_timestamp: Option<Timestamp>,
    ) -> JournalResult<EventId> {
        let event_action = payload.event_action();

        // creation events should go through the `append_creation_event_in_tx` function
        assert_eq!(event_action, EventAction::Created);

        let journal_id = payload.journal_id();
        let stream_id = payload.stream_id();
        let stream_type = payload.stream_type();
        let transaction_id = payload.transaction_id();
        let donation_id = payload.donation_id();
        let version = 0;

        let event_id = sqlx::query_scalar!(
            r#"
            INSERT INTO journal_events (journal_id, stream_type, stream_id, version, event_type, payload, user_id, authority, timestamp, display_timestamp) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING event_id
            "#,
            journal_id as JournalId,
            stream_type as i16,
            stream_id as Ident,
            version,
            payload.event_type() as i16,
            payload as JournalDomainEvent,
            user_id as Option<UserId>,
            authority as Authority,
            timestamp as Timestamp,
            display_timestamp as Option<Timestamp>
        ).fetch_one(&mut **tx)
            .await
            .map_err(|e| handle_unique_violation(e, stream_type, stream_id))?;

        let stream_prefix = stream_type.stream_prefix();

        // insert into the stream
        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new("INSERT INTO ");

        builder.push(stream_prefix);
        builder.push("_stream ");

        if matches!(stream_type, StreamType::Journal) {
            builder.push("(journal_id, version, valid) VALUES (");
            let mut sep = builder.separated(", ");
            sep.push_bind(journal_id);
            sep.push_bind(0);
            sep.push_bind(true);
            sep.push_unseparated(")");
        } else {
            builder.push("(journal_id, version, valid, ");
            builder.push(stream_prefix);
            builder.push("_id");

            if let Some(_transaction_id) = transaction_id {
                builder.push(", transaction_id");
            } else if let Some(_donation_id) = donation_id {
                builder.push(", donation_id");
            }

            builder.push(") VALUES (");

            let mut sep = builder.separated(", ");
            sep.push_bind(journal_id);
            sep.push_bind(0);
            sep.push_bind(true);
            sep.push_bind(stream_id);
            if let Some(transaction_id) = transaction_id {
                sep.push_bind(transaction_id);
            } else if let Some(donation_id) = donation_id {
                sep.push_bind(donation_id);
            }
            sep.push_unseparated(")");
        }

        builder.build().execute(&mut **tx).await?;

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

    /// push the event, and notify the listener, and update the stream
    #[allow(clippy::too_many_arguments)]
    pub async fn append_modification_event_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        stream_handle: StreamHandle,
        payload: JournalDomainEvent,
        user_id: Option<UserId>,
        authority: Authority,
        timestamp: Timestamp,
        display_timestamp: Option<Timestamp>,
    ) -> JournalResult<EventId> {
        let event_action = payload.event_action();

        // creation events should go through the `append_creation_event_in_tx` function
        assert_ne!(event_action, EventAction::Created);

        let journal_id = stream_handle.journal_id;
        let stream_id = stream_handle.stream_id;
        let stream_type = stream_handle.stream_type;
        let version = stream_handle.version + 1;

        // ensure the stream handle matches the payload info
        assert_eq!(payload.journal_id(), journal_id);
        assert_eq!(payload.stream_type(), stream_type);
        assert_eq!(payload.stream_id(), stream_id);

        let event_id = sqlx::query_scalar!(
            r#"
            INSERT INTO journal_events (journal_id, stream_type, stream_id, version, event_type, payload, user_id, authority, timestamp, display_timestamp) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING event_id
            "#,
            journal_id as JournalId,
            stream_type as i16,
            stream_id as Ident,
            version,
            payload.event_type() as i16,
            payload as JournalDomainEvent,
            user_id as Option<UserId>,
            authority as Authority,
            timestamp as Timestamp,
            display_timestamp as Option<Timestamp>
        ).fetch_one(&mut **tx)
            .await
            .map_err(|e| handle_unique_violation(e, stream_type, stream_id))?;

        let stream_prefix = stream_type.stream_prefix();

        // update the relevant stream
        let mut builder: QueryBuilder<Postgres> = QueryBuilder::new("UPDATE ");

        builder.push(stream_prefix);
        builder.push("_stream ");

        builder.push("SET version = ");
        builder.push_bind(version);

        if matches!(event_action, EventAction::Deleted) {
            builder.push(", valid = false");
        }

        builder.push(" WHERE journal_id = ");
        builder.push_bind(journal_id);

        if !matches!(stream_type, StreamType::Journal) {
            builder.push(" AND ");
            builder.push(stream_prefix);
            builder.push("_id=");
            builder.push_bind(stream_id);
        }

        builder.build().execute(&mut **tx).await?;

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
}

#[derive(Clone)]
pub struct JournalService {
    pub pool: PgPool,
    pub object_store: ObjectStore,
    pub projection: &'static JournalProjection,
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
    Donor = 8,
    DonationEntry = 9,
    Donation = 10,
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum EventAction {
    Created,
    Modified,
    Deleted,
}

impl StreamType {
    pub const fn stream_prefix(&self) -> &'static str {
        match self {
            StreamType::Journal => "journal",
            StreamType::Account => "account",
            StreamType::Activity => "activity",
            StreamType::Fund => "fund",
            StreamType::Entry => "entry",
            StreamType::Transaction => "transaction",
            StreamType::File => "file",
            StreamType::Donor => "donor",
            StreamType::DonationEntry => "donation_entry",
            StreamType::Donation => "donation",
        }
    }
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
    DonorCreated = 801,
    DonationEntryCreated = 901,
    DonationCreated = 1001,
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
            StreamType::Donor => JournalError::DonorIdCollision(DonorId(stream_id)),
            StreamType::DonationEntry => {
                JournalError::DonationEntryIdCollision(DonationEntryId(stream_id))
            }
            StreamType::Donation => JournalError::DonationIdCollision(DonationId(stream_id)),
        };
    }
    JournalError::from(error)
}

fn handle_missing_stream(stream_id: Ident, stream_type: StreamType) -> JournalError {
    use JournalError::*;

    match stream_type {
        StreamType::Journal => InvalidJournal(JournalId(stream_id)),
        StreamType::Account => InvalidAccount(AccountId(stream_id)),
        StreamType::Activity => InvalidActivity(ActivityId(stream_id)),
        StreamType::Fund => InvalidFund(FundId(stream_id)),
        StreamType::Entry => InvalidEntry(EntryId(stream_id)),
        StreamType::Transaction => InvalidTransaction(TransactionId(stream_id)),
        StreamType::File => InvalidFile(FileId(stream_id)),
        StreamType::Donor => InvalidDonor(DonorId(stream_id)),
        StreamType::DonationEntry => InvalidDonationEntry(DonationEntryId(stream_id)),
        StreamType::Donation => InvalidDonation(DonationId(stream_id)),
    }
}

pub(crate) enum MemberPayload {
    CreatedJournal,
    AddedToJournal(Permissions),
    PermissionsUpdated(Permissions),
    RemovedFromJournal,
}

pub(crate) async fn get_latest_member_payload(
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
    pub async fn try_new(pool: PgPool) -> Result<Self, sqlx::Error> {
        sqlx::migrate!("./migrations").run(&pool).await?;

        let projection = Box::leak(Box::new(JournalProjection::new(pool.clone())));

        tokio::spawn(
            projection
                .run()
                .inspect_err(|e| error!("projection failed: {:#?}", e)),
        );
        tokio::spawn(
            projection
                .update_last_processed_event()
                .inspect_err(|e| error!("event id processor failed: {:#?}", e)),
        );

        Ok(Self {
            pool,
            object_store: ObjectStore::new().await,
            projection,
        })
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
    pub(crate) async fn check_permissions_in_tx(
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
            return Err(InvalidJournal(journal_id));
        }

        match authority.actor() {
            Actor::User(user_id) => {
                let payload = get_latest_member_payload(tx, journal_id, user_id)
                    .await?
                    .ok_or(InvalidJournal(journal_id))?;
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
                    MemberPayload::RemovedFromJournal => Err(InvalidJournal(journal_id)),
                }
            }
            Actor::System => Ok(()),
            Actor::Anonymous => Err(JournalError::Permissions(expected_permissions)),
        }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn object_store(&self) -> &ObjectStore {
        &self.object_store
    }

    pub async fn wait_for(&self, event_id: EventId) {
        self.projection.wait_for(event_id).await
    }
}

pub async fn handle_incoming_event(
    payload: JournalDomainEvent,
    tx: &mut PgTransaction<'_>,
) -> Result<(), sqlx::Error> {
    match payload {
        JournalDomainEvent::JournalCreated {
            journal_id,
            owner,
            name,
        } => {
            sqlx::query!(
                    r#"
                    INSERT INTO journals (id, owner_id, name) VALUES($1, $2, $3) ON CONFLICT DO NOTHING
                    "#,
                    journal_id as JournalId,
                    owner as UserId,
                    name as Name
                )
                .execute(&mut **tx)
                .await?;
        }
        JournalDomainEvent::JournalDeleted { journal_id, .. } => {
            sqlx::query!(
                r#"
                    DELETE FROM journals where id = $1
                    "#,
                journal_id as JournalId
            )
            .execute(&mut **tx)
            .await?;
        }
        JournalDomainEvent::MemberAdded {
            journal_id,
            user_id,
            permissions,
        } => {
            sqlx::query!(
                    r#"
                    INSERT INTO journal_members (user_id, journal_id, permissions) VALUES($1, $2, $3) ON CONFLICT DO NOTHING
                    "#,
                    user_id as UserId,
                    journal_id as JournalId,
                    permissions as Permissions
                    )
                .execute(&mut **tx)
                .await?;
        }
        JournalDomainEvent::MemberPermissionsUpdated {
            journal_id,
            user_id,
            permissions,
        } => {
            sqlx::query!(
                    r#"
                    UPDATE journal_members SET permissions = $1 WHERE user_id = $2 AND journal_id = $3
                    "#,
                    permissions as Permissions,
                    user_id as UserId,
                    journal_id as JournalId,
                )
                .execute(&mut **tx)
                .await?;
        }
        JournalDomainEvent::MemberRemoved {
            journal_id,
            user_id,
        } => {
            sqlx::query!(
                r#"
                    DELETE FROM journal_members WHERE user_id = $1 AND journal_id = $2
                    "#,
                user_id as UserId,
                journal_id as JournalId,
            )
            .execute(&mut **tx)
            .await?;
        }
        JournalDomainEvent::AccountCreated {
            account_id,
            journal_id,
            name,
            account_type,
        } => {
            sqlx::query!(
                    r#"
                    INSERT INTO accounts (id, journal_id, name, balance, account_type) VALUES($1, $2, $3, 0, $4) ON CONFLICT DO NOTHING
                    "#,
                    account_id as AccountId,
                    journal_id as JournalId,
                    name as Name,
                    account_type as AccountType,
                )
                .execute(&mut **tx)
                .await?;
        }
        JournalDomainEvent::AccountRenamed {
            account_id,
            journal_id,
            new_name,
        } => {
            sqlx::query!(
                r#"
                    UPDATE accounts SET name = $1 WHERE id = $2 AND journal_id = $3
                    "#,
                new_name as Name,
                account_id as AccountId,
                journal_id as JournalId,
            )
            .execute(&mut **tx)
            .await?;
        }
        JournalDomainEvent::AccountDeleted {
            account_id,
            journal_id,
        } => {
            sqlx::query!(
                r#"
                    DELETE FROM accounts WHERE id = $1 AND journal_id = $2
                    "#,
                account_id as AccountId,
                journal_id as JournalId,
            )
            .execute(&mut **tx)
            .await?;
        }
        JournalDomainEvent::TransactionCreated {
            transaction_id,
            journal_id,
            memo,
            financial_period,
        } => {
            sqlx::query!(
                    r#"
                    INSERT INTO transactions (id, journal_id, financial_period, memo) VALUES($1, $2, $3, $4) ON CONFLICT DO NOTHING
                    "#,
                    transaction_id as TransactionId,
                    journal_id as JournalId,
                    financial_period as FinancialPeriod,
                    memo as Option<Memo>
                )
                .execute(&mut **tx)
                .await?;
        }
        JournalDomainEvent::FileUploaded {
            file_id,
            journal_id,
            hash,
            file_name,
        } => {
            sqlx::query!(
                    r#"
                    INSERT INTO files (id, journal_id, hash, name) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING
                    "#,
                    file_id as FileId,
                    journal_id as JournalId,
                    hash.as_slice() as &[u8],
                    file_name
                )
                .execute(&mut **tx)
                .await?;
        }
        JournalDomainEvent::FileDeleted {
            file_id,
            journal_id,
        } => {
            sqlx::query!(
                r#"
                    DELETE FROM files where id = $1 AND journal_id = $2
                    "#,
                file_id as FileId,
                journal_id as JournalId,
            )
            .execute(&mut **tx)
            .await?;
        }
        JournalDomainEvent::FundCreated {
            fund_id,
            journal_id,
            fund_name,
        } => {
            sqlx::query!(
                    r#"
                    INSERT INTO funds (id, journal_id, name, balance) VALUES ($1, $2, $3, 0) ON CONFLICT DO NOTHING
                    "#,
                    fund_id as FundId,
                    journal_id as JournalId,
                    fund_name as Name
                )
                .execute(&mut **tx)
                .await?;
        }
        JournalDomainEvent::EntryCreated {
            entry_id,
            journal_id,
            transaction_id,
            amount,
            entry_side,
            entry_kind,
        } => {
            sqlx::query!(
                    r#"
                    INSERT INTO entries (id, journal_id, transaction_id, amount, entry_side, entry_kind) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT DO NOTHING
                    "#,
                    entry_id as EntryId,
                    journal_id as JournalId,
                    transaction_id as TransactionId,
                    amount as i64,
                    entry_side as EntrySide,
                    entry_kind as EntryKind
                )
                .execute(&mut **tx)
                .await?;

            let diff = match entry_side {
                EntrySide::Credit => amount as i64,
                EntrySide::Debit => -(amount as i64),
            };

            match entry_kind {
                EntryKind::Account { account_id } => {
                    sqlx::query!(
                            r#"
                            UPDATE accounts SET balance = balance + $1 WHERE id = $2 AND journal_id = $3
                            "#,
                            diff,
                            account_id as AccountId,
                            journal_id as JournalId,
                        ).execute(&mut **tx).await?;
                }
                EntryKind::Activity {
                    activity_id,
                    fund_id,
                    scope: _,
                } => {
                    sqlx::query!(
                            r#"
                            UPDATE funds SET balance = balance + $1 WHERE id = $2 AND journal_id = $3;
                            "#,
                            diff,
                            fund_id as FundId,
                            journal_id as JournalId,
                        ).execute(&mut **tx).await?;
                    sqlx::query!(
                            r#"
                            UPDATE activities SET balance = balance + $1 WHERE id = $2 AND journal_id = $3;
                            "#,
                            diff,
                            activity_id as ActivityId,
                            journal_id as JournalId,
                        ).execute(&mut **tx).await?;
                }
            }
        }
        JournalDomainEvent::ActivityCreated {
            activity_id,
            journal_id,
            activity_name,
            activity_kind: activity_type,
        } => {
            sqlx::query!(
                    r#"
                    INSERT INTO activities (id, journal_id, name, balance, activity_kind) VALUES ($1, $2, $3, 0, $4) ON CONFLICT DO NOTHING
                    "#,
                    activity_id as ActivityId,
                    journal_id as JournalId,
                    activity_name as Name,
                    activity_type as i16
                )
                .execute(&mut **tx)
                .await?;
        }
        JournalDomainEvent::DonorCreated {
            donor_id,
            journal_id,
            name,
            ..
        } => {
            sqlx::query!(
                r#"
                INSERT INTO donors (id, journal_id, name, total_donated) VALUES ($1, $2, $3, 0) ON CONFLICT DO NOTHING
                "#,
                donor_id as DonorId,
                journal_id as JournalId,
                name as Name,
            ).execute(&mut **tx).await?;
        }
        JournalDomainEvent::DonationEntryCreated {
            donation_entry_id,
            journal_id,
            fund_id,
            donor_id,
            donation_id,
            amount,
            instrument,
            check_num,
        } => {
            sqlx::query!(
                r#"
                INSERT INTO donation_entries (id, journal_id, donor_id, fund_id, donation_id, amount, instrument, check_num) VALUES ($1, $2, $3, $4, $5, $6, $7, $8) ON CONFLICT DO NOTHING
                "#,
                donation_entry_id as DonationEntryId,
                journal_id as JournalId,
                donor_id as DonorId,
                fund_id as FundId,
                donation_id as DonationId,
                amount as i64,
                instrument as DonationInstrument,
                check_num as Option<CheckNum>
            ).execute(&mut **tx).await?;

            // update the activity's balance
            sqlx::query!(
                r#"
                UPDATE funds SET balance = balance + $1 WHERE id = $2 AND journal_id = $3;
                "#,
                amount as i64,
                fund_id as FundId,
                journal_id as JournalId,
            )
            .execute(&mut **tx)
            .await?;
        }
        JournalDomainEvent::DonationCreated {
            donation_id,
            journal_id,
            ..
        } => {
            sqlx::query!(
                r#"
                INSERT INTO donations (id, journal_id) VALUES ($1, $2) ON CONFLICT DO NOTHING
                "#,
                donation_id as DonationId,
                journal_id as JournalId
            )
            .execute(&mut **tx)
            .await?;
        }
    }

    Ok(())
}
