use crate::authn::UserId;
use crate::authority::{Actor, Authority};
use crate::id::Ident;
use crate::journal::account::AccountId;
use crate::journal::activity::ActivityId;
use crate::journal::donation::DonationId;
use crate::journal::donation::donation_entry::DonationEntryId;
use crate::journal::donation::donor::DonorId;
use crate::journal::error::JournalError::InvalidJournal;
use crate::journal::error::{JournalError, JournalResult};
use crate::journal::event::JournalDomainEvent;
use crate::journal::file::{FileId, ObjectStore};
use crate::journal::fund::FundId;
use crate::journal::projection;
use crate::journal::transaction::TransactionId;
use crate::journal::transaction::entry::EntryId;
use crate::journal::{JournalId, Permissions};
use axum_login::tracing::error;
use futures_util::TryFutureExt;
use sqlx::postgres::PgListener;
use sqlx::{PgPool, Postgres, Transaction};
use thiserror::Error;
use tokio::sync::broadcast;

pub type EventId = i64;
pub type Version = i64;

// the methods for getting and using a StreamHandle are placed in a
// submodule to prevent accidental fabrication
pub(crate) mod stream_lock {
    use crate::authn::UserId;
    use crate::authority::Authority;
    use crate::id::Ident;
    use crate::journal::JournalId;
    use crate::journal::account::AccountId;
    use crate::journal::activity::ActivityId;
    use crate::journal::donation::DonationId;
    use crate::journal::donation::donation_entry::DonationEntryId;
    use crate::journal::donation::donor::DonorId;
    use crate::journal::error::JournalResult;
    use crate::journal::event::JournalDomainEvent;
    use crate::journal::file::FileId;
    use crate::journal::fund::FundId;
    use crate::journal::service::{
        EventAction, EventId, EventType, StreamType, Version, handle_missing_stream,
        handle_unique_violation,
    };
    use crate::journal::transaction::TransactionId;
    use crate::journal::transaction::entry::EntryId;
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

    /// Append the event and update its stream in the same transaction.
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

        Ok(event_id)
    }

    pub async fn batch_append_creation_event_in_tx(
        tx: &mut Transaction<'_, Postgres>,
        events: impl Iterator<
            Item = (
                JournalDomainEvent,
                Option<UserId>,
                Authority,
                Timestamp,
                Option<Timestamp>,
            ),
        >,
    ) -> JournalResult<(EventId, EventId)> {
        let mut journal_streams: Vec<JournalId> = Vec::new();
        let mut account_streams: Vec<(JournalId, AccountId)> = Vec::new();
        let mut fund_streams: Vec<(JournalId, FundId)> = Vec::new();
        let mut activity_streams: Vec<(JournalId, ActivityId)> = Vec::new();
        let mut entry_streams: Vec<(JournalId, TransactionId, EntryId)> = Vec::new();
        let mut transaction_streams: Vec<(JournalId, TransactionId)> = Vec::new();
        let mut file_streams: Vec<(JournalId, FileId)> = Vec::new();
        let mut donor_streams: Vec<(JournalId, DonorId)> = Vec::new();
        let mut donation_entry_streams: Vec<(JournalId, DonationId, DonationEntryId)> = Vec::new();
        let mut donation_streams: Vec<(JournalId, DonationId)> = Vec::new();

        let mut event_builder: QueryBuilder<Postgres> = QueryBuilder::new(
            "INSERT INTO journal_events (journal_id, stream_type, stream_id, version, event_type, payload, user_id, authority, timestamp, display_timestamp) ",
        );
        event_builder.push_values(
            events,
            |mut row, (payload, user_id, authority, timestamp, display_timestamp)| {
                assert_eq!(payload.event_action(), EventAction::Created);

                match payload.event_type() {
                    EventType::JournalCreated => {
                        journal_streams.push(JournalId(payload.stream_id()))
                    }
                    EventType::AccountCreated => {
                        account_streams.push((payload.journal_id(), AccountId(payload.stream_id())))
                    }
                    EventType::FundCreated => {
                        fund_streams.push((payload.journal_id(), FundId(payload.stream_id())))
                    }
                    EventType::ActivityCreated => activity_streams
                        .push((payload.journal_id(), ActivityId(payload.stream_id()))),
                    EventType::EntryCreated => entry_streams.push((
                        payload.journal_id(),
                        payload
                            .transaction_id()
                            .expect("entry created events must have a transaction id"),
                        EntryId(payload.stream_id()),
                    )),
                    EventType::TransactionCreated => transaction_streams
                        .push((payload.journal_id(), TransactionId(payload.stream_id()))),
                    EventType::FileUploaded => {
                        file_streams.push((payload.journal_id(), FileId(payload.stream_id())))
                    }
                    EventType::DonorCreated => {
                        donor_streams.push((payload.journal_id(), DonorId(payload.stream_id())))
                    }
                    EventType::DonationEntryCreated => donation_entry_streams.push((
                        payload.journal_id(),
                        payload
                            .donation_id()
                            .expect("donation entry events must have a donation id"),
                        DonationEntryId(payload.stream_id()),
                    )),
                    EventType::DonationCreated => donation_streams
                        .push((payload.journal_id(), DonationId(payload.stream_id()))),

                    EventType::JournalDeleted => unreachable!(),
                    // member events are journal modification events
                    EventType::MemberAdded => unreachable!(),
                    EventType::MemberPermissionsUpdated => unreachable!(),
                    EventType::MemberRemoved => unreachable!(),
                    EventType::AccountRenamed => unreachable!(),
                    EventType::AccountDeleted => unreachable!(),
                    EventType::FileDeleted => unreachable!(),
                }

                row.push_bind(payload.journal_id());
                row.push_bind(payload.stream_type() as i16);
                row.push_bind(payload.stream_id());
                row.push_bind(0);
                row.push_bind(payload.event_type() as i16);
                row.push_bind(payload);
                row.push_bind(user_id);
                row.push_bind(authority);
                row.push_bind(timestamp);
                row.push_bind(display_timestamp);
            },
        );
        event_builder.push(" RETURNING event_id");

        let event_ids: Vec<EventId> = event_builder
            .build_query_scalar()
            .fetch_all(&mut **tx)
            .await?;

        if !journal_streams.is_empty() {
            let mut journal_builder: QueryBuilder<Postgres> =
                QueryBuilder::new("INSERT INTO journal_stream (journal_id, version, valid) ");
            journal_builder.push_values(journal_streams, |mut row, journal_id| {
                row.push_bind(journal_id);
                row.push_bind(0);
                row.push_bind(true);
            });
            journal_builder.build().execute(&mut **tx).await?;
        }

        if !account_streams.is_empty() {
            let mut account_builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO account_stream (journal_id, account_id, version, valid) ",
            );
            account_builder.push_values(account_streams, |mut row, (journal_id, account_id)| {
                row.push_bind(journal_id);
                row.push_bind(account_id);
                row.push_bind(0);
                row.push_bind(true);
            });
            account_builder.build().execute(&mut **tx).await?;
        }

        if !fund_streams.is_empty() {
            let mut fund_builder: QueryBuilder<Postgres> =
                QueryBuilder::new("INSERT INTO fund_stream (journal_id, fund_id, version, valid) ");
            fund_builder.push_values(fund_streams, |mut row, (journal_id, fund_id)| {
                row.push_bind(journal_id);
                row.push_bind(fund_id);
                row.push_bind(0);
                row.push_bind(true);
            });
            fund_builder.build().execute(&mut **tx).await?;
        }

        if !activity_streams.is_empty() {
            let mut activity_builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO activity_stream (journal_id, activity_id, version, valid) ",
            );
            activity_builder.push_values(activity_streams, |mut row, (journal_id, activity_id)| {
                row.push_bind(journal_id);
                row.push_bind(activity_id);
                row.push_bind(0);
                row.push_bind(true);
            });
            activity_builder.build().execute(&mut **tx).await?;
        }

        if !entry_streams.is_empty() {
            let mut entry_builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO entry_stream (journal_id, transaction_id, entry_id, version, valid) ",
            );
            entry_builder.push_values(
                entry_streams,
                |mut row, (journal_id, transaction_id, entry_id)| {
                    row.push_bind(journal_id);
                    row.push_bind(transaction_id);
                    row.push_bind(entry_id);
                    row.push_bind(0);
                    row.push_bind(true);
                },
            );
            entry_builder.build().execute(&mut **tx).await?;
        }

        if !transaction_streams.is_empty() {
            let mut transaction_builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO transaction_stream (journal_id, transaction_id, version, valid) ",
            );
            transaction_builder.push_values(
                transaction_streams,
                |mut row, (journal_id, transaction_id)| {
                    row.push_bind(journal_id);
                    row.push_bind(transaction_id);
                    row.push_bind(0);
                    row.push_bind(true);
                },
            );
            transaction_builder.build().execute(&mut **tx).await?;
        }

        if !file_streams.is_empty() {
            let mut file_builder: QueryBuilder<Postgres> =
                QueryBuilder::new("INSERT INTO file_stream (journal_id, file_id, version, valid) ");
            file_builder.push_values(file_streams, |mut row, (journal_id, file_id)| {
                row.push_bind(journal_id);
                row.push_bind(file_id);
                row.push_bind(0);
                row.push_bind(true);
            });
            file_builder.build().execute(&mut **tx).await?;
        }

        if !donor_streams.is_empty() {
            let mut donor_builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO donor_stream (journal_id, donor_id, version, valid) ",
            );
            donor_builder.push_values(donor_streams, |mut row, (journal_id, donor_id)| {
                row.push_bind(journal_id);
                row.push_bind(donor_id);
                row.push_bind(0);
                row.push_bind(true);
            });
            donor_builder.build().execute(&mut **tx).await?;
        }

        if !donation_entry_streams.is_empty() {
            let mut donation_entry_builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO donation_entry_stream (journal_id, donation_id, donation_entry_id, version, valid) ",
            );
            donation_entry_builder.push_values(
                donation_entry_streams,
                |mut row, (journal_id, donation_id, donation_entry_id)| {
                    row.push_bind(journal_id);
                    row.push_bind(donation_id);
                    row.push_bind(donation_entry_id);
                    row.push_bind(0);
                    row.push_bind(true);
                },
            );
            donation_entry_builder.build().execute(&mut **tx).await?;
        }

        if !donation_streams.is_empty() {
            let mut donation_builder: QueryBuilder<Postgres> = QueryBuilder::new(
                "INSERT INTO donation_stream (journal_id, donation_id, version, valid) ",
            );
            donation_builder.push_values(donation_streams, |mut row, (journal_id, donation_id)| {
                row.push_bind(journal_id);
                row.push_bind(donation_id);
                row.push_bind(0);
                row.push_bind(true);
            });
            donation_builder.build().execute(&mut **tx).await?;
        };

        let mut ids = event_ids.into_iter();
        let first_id = ids
            .next()
            .expect("a bulk append must insert at least one event");
        Ok(ids.fold((first_id, first_id), |(min, max), id| {
            (min.min(id), max.max(id))
        }))
    }

    /// Append the event and update its stream in the same transaction.
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

        Ok(event_id)
    }
}

#[derive(Clone)]
pub struct JournalService {
    pub pool: PgPool,
    pub object_store: ObjectStore,
    pub(super) projection_applications: broadcast::Sender<JournalId>,
    pub(crate) jewel_imports: crate::journal::jewel::jobs::ImportJobs,
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
        projection::install_sql(&pool).await?;

        let mut listener = PgListener::connect_with(&pool).await?;
        listener.listen(projection::APPLICATION_CHANNEL).await?;
        let (projection_applications, _) = broadcast::channel(128);
        tokio::spawn(projection::forward_applications(
            listener,
            projection_applications.clone(),
        ));

        let worker_pool = pool.clone();
        tokio::spawn(async move {
            projection::run(worker_pool)
                .inspect_err(|e| error!("projection failed: {:#?}", e))
                .await
        });

        Ok(Self {
            pool,
            object_store: ObjectStore::new().await,
            projection_applications,
            jewel_imports: Default::default(),
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
}
