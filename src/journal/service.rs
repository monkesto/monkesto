use crate::authn::AuthConnectError;
use crate::authn::user::UserId;
use crate::authority::{Actor, Authority};
use crate::event_id::GetEventId;
use crate::journal::JournalId;
use crate::journal::JournalResult;
use crate::journal::PermissionDecodeError;
use crate::journal::Permissions;
use crate::journal::account::{AccountId, AccountType, CreateAccount};
use crate::journal::activity::{ActivityId, ActivityType, CreateActivity};
use crate::journal::domain::JournalDomainEvent;
use crate::journal::entry::{EntryId, EntryKind, EntrySide};
use crate::journal::file::{FileId, UploadFile};
use crate::journal::fund::{CreateFund, FundId};
use crate::journal::member::{AddJournalMember, RemoveJournalMember, UpdateJournalMember};
use crate::journal::store::JournalEventStore;
use crate::journal::transaction::{
    CreateTransaction, FinancialPeriod, TransactionEntries, TransactionEntry, TransactionEntryIds,
    TransactionId,
};
use crate::journal::{CreateJournal, JournalError};
use crate::name::Name;
use crate::time_provider::Timestamp;
use async_trait::async_trait;
use axum_test::expect_json::__private::serde_trampoline::{Deserialize, Serialize};
use disintegrate::serde::prost::Prost;
use disintegrate::{DecisionError, EventListener, PersistedEvent, StreamQuery, query};
use disintegrate_postgres::{
    PgDecisionMaker, PgEventId, PgSnapshotter, WithPgSnapshot, decision_maker,
};
use prost::Message;
use proto::event::journal::ProtoJournalDomainEvent;
use sqlx::{FromRow, PgPool};
use std::collections::HashMap;
use tokio::sync::watch;

type PgJournalDecisionMaker = PgDecisionMaker<
    JournalDomainEvent,
    Prost<JournalDomainEvent, ProtoJournalDomainEvent>,
    WithPgSnapshot,
>;

#[derive(Copy, Clone, Serialize, Deserialize, Debug, PartialEq, FromRow)]
pub struct StoredTransactionEntry {
    pub id: EntryId,
    pub journal_id: JournalId,
    pub amount: i64,
    pub entry_side: EntrySide,
    pub entry_kind: EntryKind,
}

pub struct JournalState {
    pub id: JournalId,
    pub owner_id: UserId,
    pub name: Name,
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

#[expect(unused)]
pub struct ActivityState {
    pub id: ActivityId,
    pub journal_id: JournalId,
    pub name: Name,
    pub activity_type: ActivityType,
    pub balance: i64,
}

#[expect(unused)]
pub struct FundState {
    pub id: FundId,
    pub journal_id: JournalId,
    pub name: Name,
    pub balance: i64,
}

pub struct TransactionState {
    pub id: TransactionId,
    #[expect(unused)]
    pub journal_id: JournalId,
    pub entries: Vec<StoredTransactionEntry>,
}

pub struct FileState {
    pub id: FileId,
    #[expect(unused)]
    journal_id: JournalId,
    #[expect(unused)]
    pub hash: [u8; 16],
    pub name: String,
}

#[derive(FromRow)]
struct JournalStateWithPayload {
    id: JournalId,
    owner_id: UserId,
    name: Name,
    payload: Vec<u8>,
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

#[derive(FromRow)]
pub struct ActivityStateWithPayload {
    id: ActivityId,
    #[expect(unused)]
    journal_id: JournalId,
    name: Name,
    activity_type: ActivityType,
    balance: i64,
    payload: Vec<u8>,
}

#[derive(FromRow)]
pub struct FundStateWithPayload {
    id: FundId,
    #[expect(unused)]
    journal_id: JournalId,
    name: Name,
    balance: i64,
    payload: Vec<u8>,
}
#[derive(FromRow)]
struct TransactionStateWithPayload {
    id: TransactionId,
    #[expect(unused)]
    journal_id: JournalId,
    entries: TransactionEntryIds,
    payload: Vec<u8>,
}

#[derive(FromRow)]
struct FileStateWithPayload {
    id: FileId,
    #[expect(unused)]
    journal_id: JournalId,
    hash: Vec<u8>,
    name: String,
    payload: Vec<u8>,
}

#[derive(FromRow)]
struct FileStateWithVecHash {
    id: FileId,
    journal_id: JournalId,
    hash: Vec<u8>,
    name: String,
}

#[derive(Clone)]
pub struct JournalService {
    query: StreamQuery<PgEventId, JournalDomainEvent>,
    projection_pool: PgPool,
    decision_maker: PgJournalDecisionMaker,
    current_event: watch::Sender<PgEventId>,
}

impl JournalService {
    pub async fn try_new(
        pool: PgPool,
        event_store: JournalEventStore,
    ) -> Result<Self, AuthConnectError> {
        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS journals (
                id TEXT PRIMARY KEY,
                owner_id TEXT NOT NULL,
                name TEXT NOT NULL
            )
        "#
        )
        .execute(&pool)
        .await?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS journal_members (
                user_id TEXT NOT NULL,
                journal_id TEXT NOT NULL,
                permissions INTEGER NOT NULL
            )
        "#
        )
        .execute(&pool)
        .await?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS accounts (
                id TEXT PRIMARY KEY,
                journal_id TEXT NOT NULL,
                name TEXT NOT NULL,
                account_type int2,
                balance BIGINT NOT NULL
            )
        "#
        )
        .execute(&pool)
        .await?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS transactions (
                id TEXT PRIMARY KEY,
                journal_id TEXT NOT NULL,
                entries BYTEA NOT NULL
            )
        "#
        )
        .execute(&pool)
        .await?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS files (
                id TEXT PRIMARY KEY,
                journal_id TEXT NOT NULL,
                hash BYTEA NOT NULL,
                name TEXT NOT NULL
            )
        "#
        )
        .execute(&pool)
        .await?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS activities (
            id TEXT NOT NULL,
            journal_id TEXT NOT NULL,
            name TEXT NOT NULL,
            activity_type int2 NOT NULL,
            balance BIGINT NOT NULL
        )
        "#
        )
        .execute(&pool)
        .await?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS funds (
                id TEXT PRIMARY KEY,
                journal_id TEXT NOT NULL,
                name TEXT NOT NULL,
                balance BIGINT NOT NULL
            )
        "#
        )
        .execute(&pool)
        .await?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS transaction_entries (
                id TEXT NOT NULL,
                journal_id TEXT NOT NULL,
                amount BIGINT NOT NULL,
                entry_side int2 NOT NULL,
                entry_kind BYTEA NOT NULL
            )
        "#
        )
        .execute(&pool)
        .await?;

        let snapshotter = PgSnapshotter::try_new(pool.clone(), 10)
            .await
            .expect("failed to create a snapshotter for the journal service");

        let decision_maker =
            decision_maker(event_store.event_store, WithPgSnapshot::new(snapshotter));

        let (sender, receiver) = watch::channel(0);

        Box::leak(Box::new(receiver));

        Ok(Self {
            query: query!(JournalDomainEvent),
            projection_pool: pool,
            decision_maker,
            current_event: sender,
        })
    }

    pub async fn create_journal(
        &self,
        journal_id: JournalId,
        owner: UserId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(CreateJournal::new(
                journal_id, owner, name, authority, timestamp,
            ))
            .await?
            .event_id())
    }

    pub async fn add_member(
        &self,
        journal_id: JournalId,
        member_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(AddJournalMember::new(
                journal_id,
                member_id,
                permissions,
                authority,
                timestamp,
            ))
            .await?
            .event_id())
    }

    pub async fn update_member(
        &self,
        journal_id: JournalId,
        member_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(UpdateJournalMember::new(
                journal_id,
                member_id,
                permissions,
                authority,
                timestamp,
            ))
            .await?
            .event_id())
    }

    pub async fn remove_member(
        &self,
        journal_id: JournalId,
        member_id: UserId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(RemoveJournalMember::new(
                journal_id, member_id, authority, timestamp,
            ))
            .await?
            .event_id())
    }

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

    #[expect(unused)]
    pub async fn create_fund(
        &self,
        fund_id: FundId,
        journal_id: JournalId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(CreateFund::new(
                fund_id, journal_id, name, authority, timestamp,
            ))
            .await?
            .event_id())
    }

    #[expect(unused)]
    pub async fn create_activity(
        &self,
        activity_id: ActivityId,
        journal_id: JournalId,
        name: Name,
        activity_type: ActivityType,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(CreateActivity::new(
                activity_id,
                journal_id,
                name,
                activity_type,
                authority,
                timestamp,
            ))
            .await?
            .event_id())
    }

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

    pub async fn upload_file(
        &self,
        file_id: FileId,
        journal_id: JournalId,
        hash: [u8; 16],
        file_name: String,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(UploadFile::new(
                file_id, journal_id, hash, file_name, authority, timestamp,
            ))
            .await?
            .event_id())
    }

    pub async fn get_effective_permissions(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Permissions> {
        match authority.actor() {
            Actor::System => Ok(Permissions::OWNER),
            Actor::Anonymous => Ok(Permissions::empty()),
            Actor::User(user_id) => {
                let permission_bits = sqlx::query_scalar!(
                    r#"
                    SELECT
                        CASE
                            WHEN j.owner_id = $1 THEN $2::INTEGER
                            ELSE COALESCE(
                                 (SELECT jm.permissions
                                 FROM journal_members jm
                                 WHERE jm.journal_id = j.id AND jm.user_id = $1),
                                 0
                            )
                        END as "i32!"
                    FROM journals j
                    WHERE j.id = $3
                "#,
                    *user_id as UserId,
                    Permissions::all().bits(),
                    journal_id as JournalId
                )
                .fetch_optional(&self.projection_pool)
                .await?;

                if let Some(bits) = permission_bits {
                    Ok(Permissions::from_bits(bits)
                        .ok_or(JournalError::PermissionDecode(PermissionDecodeError(bits)))?)
                } else {
                    Ok(Permissions::empty())
                }
            }
        }
    }

    /// returns the current state, creation authority, and creation timestamp of every accessible journal
    pub async fn list_accessible_journals(
        &self,
        user: UserId,
    ) -> JournalResult<Vec<(JournalState, Authority, Timestamp)>> {
        // NOTE(gabriel): a user must not be both a member and the owner, or this query will return duplicate journals

        let journals = sqlx::query_as!(
            JournalStateWithPayload,
            r#"
            SELECT j.id as "id: JournalId", j.owner_id as "owner_id: UserId", j.name as "name: Name", e.payload as "payload!"
            FROM journals j
            INNER JOIN event e
                ON e.journal_id = j.id AND e.event_type = 'JournalCreated'
            LEFT JOIN journal_members jm ON jm.journal_id = j.id AND (jm.permissions & $1) = $1
            WHERE j.owner_id = $2 OR jm.user_id = $2
            "#,
            Permissions::READ.bits(),
            user as UserId)
            .fetch_all(&self.projection_pool)
            .await?;

        // TODO(gabriel) would .map() be more efficient here?
        let mut journals_with_meta = Vec::with_capacity(journals.len());

        for journal in journals {
            let payload = JournalDomainEvent::try_from(ProtoJournalDomainEvent::decode(
                journal.payload.as_slice(),
            )?)?;

            match payload {
                JournalDomainEvent::JournalCreated {
                    authority,
                    timestamp,
                    ..
                } => {
                    journals_with_meta.push((
                        JournalState {
                            id: journal.id,
                            owner_id: journal.owner_id,
                            name: journal.name,
                        },
                        authority,
                        timestamp,
                    ));
                }
                _ => unreachable!("JournalCreated events are filtered by the sql query"),
            }
        }

        Ok(journals_with_meta)
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
            return Err(JournalError::InvalidJournal(journal_id));
        }

        let journal = sqlx::query_as!(
            JournalStateWithPayload,
            r#"
            SELECT j.id as "id: JournalId", j.owner_id as "owner_id: UserId", j.name as "name: Name", e.payload as "payload!"
            FROM journals j
            INNER JOIN event e
                ON e.journal_id = $1 AND e.event_type = 'JournalCreated'
            WHERE j.id = $1
            "#,
            journal_id as JournalId)
            .fetch_optional(&self.projection_pool)
            .await?;

        if let Some(journal) = journal {
            let payload = JournalDomainEvent::try_from(ProtoJournalDomainEvent::decode(
                journal.payload.as_slice(),
            )?)?;

            match payload {
                JournalDomainEvent::JournalCreated {
                    authority,
                    timestamp,
                    ..
                } => Ok((
                    JournalState {
                        id: journal.id,
                        owner_id: journal.owner_id,
                        name: journal.name,
                    },
                    authority,
                    timestamp,
                )),
                _ => unreachable!("JournalCreated events are filtered by the sql query"),
            }
        } else {
            Err(JournalError::InvalidJournal(journal_id))
        }
    }

    pub async fn list_journal_members(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<UserId>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(JournalError::InvalidJournal(journal_id));
        }

        Ok(sqlx::query_scalar!(
            r#"
            SELECT user_id as "user_id: UserId" FROM journal_members WHERE journal_id = $1
            "#,
            journal_id as JournalId
        )
        .fetch_all(&self.projection_pool)
        .await?)
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

    #[expect(unused)]
    pub async fn list_journal_funds(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(FundState, Authority, Timestamp)>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(JournalError::InvalidJournal(journal_id));
        }

        let funds = sqlx::query_as!(
            FundStateWithPayload,
            r#"
            SELECT f.id as "id: FundId", f.journal_id as "journal_id: JournalId", f.name as "name: Name", f.balance, e.payload as "payload!"
            FROM funds f
            INNER JOIN event e
                ON e.account_id = f.id AND e.event_type = 'FundCreated'
            WHERE e.journal_id = $1
            "#,
            journal_id as JournalId)
            .fetch_all(&self.projection_pool)
            .await?;

        let mut funds_with_meta = Vec::with_capacity(funds.len());

        for fund in funds {
            let payload = JournalDomainEvent::try_from(ProtoJournalDomainEvent::decode(
                fund.payload.as_slice(),
            )?)?;

            match payload {
                JournalDomainEvent::FundCreated {
                    authority,
                    timestamp,
                    ..
                } => {
                    funds_with_meta.push((
                        FundState {
                            id: fund.id,
                            journal_id,
                            name: fund.name,
                            balance: fund.balance,
                        },
                        authority,
                        timestamp,
                    ));
                }
                _ => unreachable!("FundCreated events are filtered by the sql query"),
            }
        }

        Ok(funds_with_meta)
    }

    #[expect(unused)]
    pub async fn list_journal_activities(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(ActivityState, Authority, Timestamp)>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(JournalError::InvalidJournal(journal_id));
        }

        let activities = sqlx::query_as!(
            ActivityStateWithPayload,
            r#"
            SELECT a.id as "id: ActivityId", a.journal_id as "journal_id: JournalId", a.name as "name: Name", a.activity_type as "activity_type: ActivityType", a.balance, e.payload as "payload!"
            FROM activities a
            INNER JOIN event e
                ON e.account_id = a.id AND e.event_type = 'ActivityCreated'
            WHERE e.journal_id = $1
            "#,
            journal_id as JournalId)
            .fetch_all(&self.projection_pool)
            .await?;

        let mut activities_with_meta = Vec::with_capacity(activities.len());

        for activity in activities {
            let payload = JournalDomainEvent::try_from(ProtoJournalDomainEvent::decode(
                activity.payload.as_slice(),
            )?)?;

            match payload {
                JournalDomainEvent::ActivityCreated {
                    authority,
                    timestamp,
                    ..
                } => {
                    activities_with_meta.push((
                        ActivityState {
                            id: activity.id,
                            journal_id,
                            name: activity.name,
                            activity_type: activity.activity_type,
                            balance: activity.balance,
                        },
                        authority,
                        timestamp,
                    ));
                }
                _ => unreachable!("ActivityCreated events are filtered by the sql query"),
            }
        }

        Ok(activities_with_meta)
    }

    pub async fn list_journal_files(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(FileState, Authority, Timestamp)>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(JournalError::InvalidJournal(journal_id));
        }

        let files = sqlx::query_as!(
            FileStateWithPayload,
            r#"
            SELECT f.id as "id: FileId", f.journal_id as "journal_id: JournalId", f.hash, f.name, e.payload as "payload!"
            FROM files f
            INNER JOIN event e
                ON e.file_id = f.id AND e.event_type = 'FileUploaded'
            WHERE f.journal_id = $1
            "#,
            journal_id as JournalId
        )
            .fetch_all(&self.projection_pool)
            .await?;

        let mut files_with_meta = Vec::with_capacity(files.len());

        for file in files {
            let payload = JournalDomainEvent::try_from(ProtoJournalDomainEvent::decode(
                file.payload.as_slice(),
            )?)?;
            let hash = *file.hash.as_array::<16>().ok_or_else(|| {
                JournalError::EventDecode(format!(
                    "expected 16 byte file hash, got {}",
                    file.hash.len()
                ))
            })?;

            match payload {
                JournalDomainEvent::FileUploaded {
                    authority,
                    timestamp,
                    ..
                } => files_with_meta.push((
                    FileState {
                        id: file.id,
                        journal_id,
                        hash,
                        name: file.name,
                    },
                    authority,
                    timestamp,
                )),
                _ => unreachable!("FileUploaded events are filtered by the sql query"),
            }
        }

        Ok(files_with_meta)
    }

    pub async fn get_file(
        &self,
        file_id: FileId,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<FileState> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(JournalError::InvalidJournal(journal_id));
        }

        let file = sqlx::query_as!(
            FileStateWithVecHash,
            r#"
            SELECT f.id as "id: FileId", f.journal_id as "journal_id: JournalId", f.hash, f.name
            FROM files f
            WHERE f.journal_id = $1 AND f.id = $2
            "#,
            journal_id as JournalId,
            file_id as FileId,
        )
        .fetch_optional(&self.projection_pool)
        .await?;

        if let Some(file) = file {
            let hash = *file.hash.as_array::<16>().ok_or_else(|| {
                JournalError::EventDecode(format!(
                    "expected 16 byte file hash, got {}",
                    file.hash.len()
                ))
            })?;

            return Ok(FileState {
                id: file.id,
                journal_id: file.journal_id,
                hash,
                name: file.name,
            });
        }

        Err(JournalError::InvalidFile(file_id))
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

    pub async fn wait_for(&self, event_id: PgEventId) {
        self.current_event
            .subscribe()
            .wait_for(|curr_id| *curr_id >= event_id)
            .await
            .expect("journal service eventid sender closed");
    }
}

#[async_trait]
impl EventListener<PgEventId, JournalDomainEvent> for JournalService {
    type Error = sqlx::Error;

    fn id(&self) -> &'static str {
        "journal store"
    }

    fn query(&self) -> &StreamQuery<PgEventId, JournalDomainEvent> {
        &self.query
    }

    async fn handle(
        &self,
        event: PersistedEvent<PgEventId, JournalDomainEvent>,
    ) -> Result<(), Self::Error> {
        let event_id = event.id();
        match event.into_inner() {
            JournalDomainEvent::JournalCreated {
                journal_id,
                owner,
                name,
                ..
            } => {
                sqlx::query!(
                    r#"
                    INSERT INTO journals (id, owner_id, name) VALUES($1, $2, $3) ON CONFLICT DO NOTHING
                    "#,
                    journal_id as JournalId,
                    owner as UserId,
                    name as Name
                )
                .execute(&self.projection_pool)
                .await?;
            }
            JournalDomainEvent::JournalDeleted { journal_id, .. } => {
                sqlx::query!(
                    r#"
                    DELETE FROM journals where id = $1
                    "#,
                    journal_id as JournalId
                )
                .execute(&self.projection_pool)
                .await?;
            }
            JournalDomainEvent::MemberAdded {
                journal_id,
                user_id,
                permissions,
                ..
            } => {
                sqlx::query!(
                    r#"
                    INSERT INTO journal_members (user_id, journal_id, permissions) VALUES($1, $2, $3) ON CONFLICT DO NOTHING
                    "#,
                    user_id as UserId,
                    journal_id as JournalId,
                    permissions as Permissions
                    )
                    .execute(&self.projection_pool)
                    .await?;
            }
            JournalDomainEvent::MemberPermissionsUpdated {
                journal_id,
                user_id,
                permissions,
                ..
            } => {
                sqlx::query!(
                    r#"
                    UPDATE journal_members SET permissions = $1 WHERE user_id = $2 AND journal_id = $3
                    "#,
                    user_id as UserId,
                    journal_id as JournalId,
                    permissions as Permissions
                    )
                    .execute(&self.projection_pool)
                    .await?;
            }
            JournalDomainEvent::MemberRemoved {
                journal_id,
                user_id,
                ..
            } => {
                sqlx::query!(
                    r#"
                    DELETE FROM journal_members WHERE user_id = $1 AND journal_id = $2
                    "#,
                    user_id as UserId,
                    journal_id as JournalId,
                )
                .execute(&self.projection_pool)
                .await?;
            }
            JournalDomainEvent::AccountCreated {
                account_id,
                journal_id,
                name,
                account_type,
                ..
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
                .execute(&self.projection_pool)
                .await?;
            }
            JournalDomainEvent::AccountRenamed {
                account_id,
                journal_id,
                new_name,
                ..
            } => {
                sqlx::query!(
                    r#"
                    UPDATE accounts SET name = $1 WHERE id = $2 AND journal_id = $3
                    "#,
                    new_name as Name,
                    account_id as AccountId,
                    journal_id as JournalId,
                )
                .execute(&self.projection_pool)
                .await?;
            }
            JournalDomainEvent::AccountDeleted {
                account_id,
                journal_id,
                ..
            } => {
                sqlx::query!(
                    r#"
                    DELETE FROM accounts WHERE id = $1 AND journal_id = $2
                    "#,
                    account_id as AccountId,
                    journal_id as JournalId,
                )
                .execute(&self.projection_pool)
                .await?;
            }
            JournalDomainEvent::TransactionCreated {
                transaction_id,
                journal_id,
                entries,
                ..
            } => {
                sqlx::query!(
                    r#"
                    INSERT INTO transactions (id, journal_id, entries) VALUES($1, $2, $3) ON CONFLICT DO NOTHING
                    "#,
                    transaction_id as TransactionId,
                    journal_id as JournalId,
                    entries.clone() as TransactionEntryIds
                )
                .execute(&self.projection_pool)
                .await?;
            }
            JournalDomainEvent::FileUploaded {
                file_id,
                journal_id,
                hash,
                file_name,
                ..
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
                .execute(&self.projection_pool)
                .await?;
            }
            JournalDomainEvent::FundCreated {
                fund_id,
                journal_id,
                fund_name,
                ..
            } => {
                sqlx::query!(
                    r#"
                    INSERT INTO funds (id, journal_id, name, balance) VALUES ($1, $2, $3, 0) ON CONFLICT DO NOTHING
                    "#,
                    fund_id as FundId,
                    journal_id as JournalId,
                    fund_name as Name
                )
                    .execute(&self.projection_pool)
                    .await?;
            }
            JournalDomainEvent::EntryCreated {
                entry_id,
                journal_id,
                amount,
                entry_side,
                entry_kind,
                ..
            } => {
                let mut tx = self.projection_pool.begin().await?;

                sqlx::query!(
                    r#"
                    INSERT INTO transaction_entries (id, journal_id, amount, entry_side, entry_kind) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING
                    "#,
                    entry_id as EntryId,
                    journal_id as JournalId,
                    amount as i64,
                    entry_side as EntrySide,
                    entry_kind as EntryKind
                )
                    .execute(&mut *tx)
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
                        ).execute(&mut *tx).await?;
                    }
                    EntryKind::Activity {
                        activity_id,
                        fund_id,
                        transfer: _,
                    } => {
                        sqlx::query!(
                            r#"
                            UPDATE funds SET balance = balance + $1 WHERE id = $2 AND journal_id = $3;
                            "#,
                            diff,
                            fund_id as FundId,
                            journal_id as JournalId,
                        ).execute(&mut *tx).await?;
                        sqlx::query!(
                            r#"
                            UPDATE activities SET balance = balance + $1 WHERE id = $2 AND journal_id = $3;
                            "#,
                            diff,
                            activity_id as ActivityId,
                            journal_id as JournalId,
                        ).execute(&mut *tx).await?;
                    }
                }
                tx.commit().await?;
            }
            JournalDomainEvent::ActivityCreated {
                activity_id,
                journal_id,
                activity_name,
                activity_type,
                ..
            } => {
                sqlx::query!(
                    r#"
                    INSERT INTO activities (id, journal_id, name, balance, activity_type) VALUES ($1, $2, $3, 0, $4) ON CONFLICT DO NOTHING
                    "#,
                    activity_id as ActivityId,
                    journal_id as JournalId,
                    activity_name as Name,
                    activity_type as i16
                )
                    .execute(&self.projection_pool)
                    .await?;
            }
        }

        self.current_event
            .send(event_id)
            .expect("journal eventid sender closed");

        Ok(())
    }
}
