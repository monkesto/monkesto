use crate::authn::UserId;
use crate::authority::{Actor, Authority};
use crate::event_id::GetEventId;
use crate::journal;
use crate::journal::JournalId;
use crate::journal::JournalResult;
use crate::journal::PermissionDecodeError;
use crate::journal::Permissions;
use crate::journal::account::{
    AccountId, AccountState, AccountStateWithPayload, AccountType, CreateAccount,
};
use crate::journal::activity::{
    ActivityId, ActivityKind, ActivityState, ActivityStateWithPayload, CreateActivity,
};
use crate::journal::entry::{EntryId, EntryKind, EntrySide};
use crate::journal::error::JournalError::InvalidJournal;
use crate::journal::event::JournalDomainEvent;
use crate::journal::event_store_revised::EventId;
use crate::journal::file::{
    DeleteFile, FileId, FileState, FileStateWithPayload, FileStateWithVecHash, ObjectStore,
    UploadFile,
};
use crate::journal::fund::{CreateFund, FundId, FundState, FundStateWithPayload};
use crate::journal::jewel::JewelImportError;
use crate::journal::jewel::extract::{JewelData, jewel_extract};
use crate::journal::member::{AddJournalMember, RemoveJournalMember, UpdateJournalMember};
use crate::journal::store::JournalEventStore;
use crate::journal::transaction::memo::Memo;
use crate::journal::transaction::{
    CreateTransaction, FinancialPeriod, StoredTransactionEntry, TransactionEntries,
    TransactionEntry, TransactionId, TransactionState, TransactionStateWithPayload,
};
use crate::journal::{CreateJournal, JournalError, JournalState, JournalStateWithPayload};
use crate::name::Name;
use crate::proto::journal::event::journal_event::ProtoJournalDomainEvent;
use crate::time::{TimeProvider, Timestamp};
use async_trait::async_trait;
use chrono::Utc;
use disintegrate::serde::prost::Prost;
use disintegrate::{EventListener, NoSnapshot, PersistedEvent, StreamQuery, query};
use disintegrate_postgres::{PgDecisionMaker, PgEventId, decision_maker};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{ConnectOptions, PgPool, PgTransaction};
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::io::Write;
use std::process::Stdio;
use std::time::Duration;
use tempfile::NamedTempFile;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::watch;

type PgJournalDecisionMaker = PgDecisionMaker<
    JournalDomainEvent,
    Prost<JournalDomainEvent, ProtoJournalDomainEvent>,
    NoSnapshot,
>;

#[async_trait]
pub trait JournalExecutor: Send + Sync {
    fn pool(&self) -> &PgPool;

    fn object_store(&self) -> &ObjectStore;

    async fn wait_for(&self, event_id: EventId);

    async fn create_journal(
        &self,
        journal_id: JournalId,
        owner: UserId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId>;

    async fn add_member(
        &self,
        journal_id: JournalId,
        member_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId>;

    async fn update_member_permissions(
        &self,
        journal_id: JournalId,
        member_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId>;

    async fn remove_member(
        &self,
        journal_id: JournalId,
        member_id: UserId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId>;

    async fn get_journal(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<(JournalState, Authority, Timestamp)>;

    /// returns the current state, creation authority, and creation timestamp of every accessible journal
    async fn list_accessible_journals(
        &self,
        user: UserId,
    ) -> JournalResult<Vec<(JournalState, Authority, Timestamp)>>;

    async fn list_journal_members(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<UserId>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(InvalidJournal(journal_id));
        }

        Ok(sqlx::query_scalar!(
            r#"
            SELECT user_id as "user_id: UserId" FROM journal_members WHERE journal_id = $1
            "#,
            journal_id as JournalId
        )
        .fetch_all(self.pool())
        .await?)
    }

    async fn get_effective_permissions(
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
                    user_id as UserId,
                    Permissions::all().bits(),
                    journal_id as JournalId
                )
                .fetch_optional(self.pool())
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

    async fn create_account(
        &self,
        account_id: AccountId,
        journal_id: JournalId,
        name: Name,
        account_type: AccountType,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId>;

    async fn list_journal_accounts(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(AccountState, Authority, Timestamp)>>;

    async fn create_fund(
        &self,
        fund_id: FundId,
        journal_id: JournalId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId>;

    async fn list_journal_funds(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(FundState, Authority, Timestamp)>>;

    async fn create_activity(
        &self,
        activity_id: ActivityId,
        journal_id: JournalId,
        name: Name,
        activity_kind: ActivityKind,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId>;

    async fn list_journal_activities(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(ActivityState, Authority, Timestamp)>>;

    #[allow(clippy::too_many_arguments)]
    async fn create_transaction(
        &self,
        transaction_id: TransactionId,
        journal_id: JournalId,
        entries: Vec<TransactionEntry>,
        period: FinancialPeriod,
        memo: Option<Memo>,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId>;

    async fn list_journal_transactions(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(TransactionState, Authority, Timestamp)>>;

    async fn upload_file(
        &self,
        file_id: FileId,
        journal_id: JournalId,
        hash: [u8; 16],
        file_name: String,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId>;

    async fn get_file(
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
        .fetch_optional(self.pool())
        .await?;

        if let Some(file) = file {
            let hash = *file
                .hash
                .as_array::<16>()
                .expect("md5 hashes should always be 16 bytes long");

            return Ok(FileState {
                id: file.id,
                journal_id: file.journal_id,
                hash,
                name: file.name,
            });
        }

        Err(JournalError::InvalidFile(file_id))
    }

    async fn list_journal_files(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(FileState, Authority, Timestamp)>>;

    async fn delete_file(
        &self,
        file_id: FileId,
        journal_id: JournalId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId>;

    async fn import_jewel_database(
        &self,
        journal_id: JournalId,
        jewel_data: JewelData,
        authority: Authority,
    ) -> JournalResult<EventId>;

    async fn get_jewel_db(
        &self,
        journal_id: JournalId,
        file_id: FileId,
        authority: Authority,
        history_length_months: u32,
        top_expense_list_size: usize,
    ) -> JournalResult<JewelData> {
        let file_key = self.get_file(file_id, journal_id, authority).await?.key();

        let (mdb_file, mdb_file_path) = match self.object_store() {
            ObjectStore::S3 {
                s3_client,
                bucket_name,
            } => {
                let mut response = s3_client
                    .get_object()
                    .bucket(bucket_name)
                    .key(file_key)
                    .send()
                    .await?;

                let mut mdb_file =
                    NamedTempFile::new().map_err(|e| JewelImportError::Io(e.to_string()))?;

                while let Some(bytes) = response
                    .body
                    .try_next()
                    .await
                    .map_err(|e| JewelImportError::Io(e.to_string()))?
                {
                    mdb_file
                        .write_all(bytes.as_ref())
                        .map_err(|e| JewelImportError::Write(e.to_string()))?;
                }

                mdb_file
                    .flush()
                    .map_err(|e| JewelImportError::Write(e.to_string()))?;

                let file_path = mdb_file.path().to_string_lossy().to_string();

                (mdb_file, file_path)
            }
            ObjectStore::Local { storage_directory } => {
                let stored_file_path = storage_directory.join(file_key);
                let mut stored_file = File::open(&stored_file_path)
                    .map_err(|e| JewelImportError::Io(e.to_string()))?;
                let mut mdb_file =
                    NamedTempFile::new().map_err(|e| JewelImportError::Io(e.to_string()))?;

                io::copy(&mut stored_file, &mut mdb_file)
                    .map_err(|e| JewelImportError::Io(e.to_string()))?;

                let file_path = mdb_file.path().to_string_lossy().to_string();

                (mdb_file, file_path)
            }
        };

        let sqlite_file = NamedTempFile::new().map_err(|e| JewelImportError::Io(e.to_string()))?;

        let sqlite_file_path = sqlite_file.path().to_string_lossy();

        let schema = Command::new("mdb-schema")
            .arg(&mdb_file_path)
            .arg("sqlite")
            .output()
            .await
            .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

        if !schema.status.success() {
            Err(JewelImportError::ChildProcessExitFailure(
                String::from_utf8_lossy(schema.stderr.as_slice()).to_string(),
            ))?;
        }

        let mut sqlite_process = Command::new("sqlite3")
            .arg(sqlite_file_path.as_ref())
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

        if let Some(mut stdin) = sqlite_process.stdin.take() {
            stdin
                .write_all(&schema.stdout)
                .await
                .map_err(|e| JewelImportError::Write(e.to_string()))?;
        }

        sqlite_process
            .wait()
            .await
            .map_err(|e| JewelImportError::Io(e.to_string()))?;

        let tables = Command::new("mdb-tables")
            .arg("-1")
            .arg(&mdb_file_path)
            .output()
            .await
            .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

        if !tables.status.success() {
            Err(JewelImportError::ChildProcessExitFailure(
                String::from_utf8_lossy(tables.stderr.as_slice()).to_string(),
            ))?;
        }

        let tables_str = String::from_utf8_lossy(&tables.stdout);

        for table in tables_str.lines() {
            let export_output = Command::new("mdb-export")
                .arg("-I")
                .arg("sqlite")
                .arg("-q")
                .arg("'")
                .arg(&mdb_file_path)
                .arg(table)
                .output()
                .await
                .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

            if !export_output.status.success() {
                Err(JewelImportError::ChildProcessExitFailure(
                    String::from_utf8_lossy(export_output.stderr.as_slice()).to_string(),
                ))?;
            }

            let mut insert_cmd = Command::new("sqlite3")
                .arg(sqlite_file_path.as_ref())
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

            if let Some(mut stdin) = insert_cmd.stdin.take() {
                stdin
                    .write_all(&export_output.stdout)
                    .await
                    .map_err(|e| JewelImportError::Write(e.to_string()))?;
            }

            insert_cmd
                .wait()
                .await
                .map_err(|e| JewelImportError::Io(e.to_string()))?;
        }

        // we no longer need the mdb file
        drop(mdb_file);

        let mut conn = SqliteConnectOptions::new()
            .filename(sqlite_file_path.as_ref())
            .connect()
            .await?;

        // old versions of jewel stored the database version as an int
        #[allow(clippy::disallowed_methods)]
        let version: f64 = sqlx::query_scalar(
            r#"
                SELECT CAST(DBVersion AS REAL) as DBVersion FROM GeneralInfo;
            "#,
        )
        .fetch_one(&mut conn)
        .await?;

        if version < 9.0 {
            Err(JewelImportError::OutdatedJewelVersion(version))?;
        }

        Ok(jewel_extract(
            &mut conn,
            FinancialPeriod::July,
            history_length_months,
            top_expense_list_size,
        )
        .await?)
    }
}

#[derive(Clone)]
pub struct JournalService {
    query: StreamQuery<PgEventId, JournalDomainEvent>,
    pub(crate) projection_pool: PgPool,
    pub(crate) decision_maker: PgJournalDecisionMaker,
    current_event: watch::Sender<PgEventId>,
    pub(crate) object_store: ObjectStore,
}

#[expect(unused)]
impl JournalService {
    pub(crate) async fn try_new(pool: PgPool) -> Result<Self, sqlx::Error> {
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
                memo TEXT
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
            activity_kind int2 NOT NULL,
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
            CREATE TABLE IF NOT EXISTS entries (
                id TEXT NOT NULL,
                journal_id TEXT NOT NULL,
                transaction_id TEXT NOT NULL,
                amount BIGINT NOT NULL,
                entry_side int2 NOT NULL,
                entry_kind BYTEA NOT NULL
            )
        "#
        )
        .execute(&pool)
        .await?;

        // let snapshotter = PgSnapshotter::try_new(pool.clone(), 10)
        //     .await
        //     .expect("failed to create a snapshotter for the journal service");

        let event_store = JournalEventStore::try_new(pool.clone())
            .await
            .expect("failed to create journal event store");
        let decision_maker = decision_maker(event_store.event_store.clone(), NoSnapshot);

        let (sender, receiver) = watch::channel(0);

        Box::leak(Box::new(receiver));

        let service = Self {
            query: query!(JournalDomainEvent),
            projection_pool: pool,
            decision_maker,
            current_event: sender,
            object_store: ObjectStore::new().await,
        };

        tokio::spawn(journal::event::event_listener(
            event_store.clone(),
            service.clone(),
        ));

        Ok(service)
    }
}

#[async_trait]
impl JournalExecutor for JournalService {
    fn pool(&self) -> &PgPool {
        &self.projection_pool
    }

    fn object_store(&self) -> &ObjectStore {
        &self.object_store
    }

    async fn wait_for(&self, event_id: PgEventId) {
        self.current_event
            .subscribe()
            .wait_for(|curr_id| *curr_id >= event_id)
            .await
            .expect("journal service eventid sender closed");
    }

    async fn create_journal(
        &self,
        journal_id: JournalId,
        owner: UserId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, JournalError> {
        Ok(self
            .decision_maker
            .make(CreateJournal::new(
                journal_id, owner, name, authority, timestamp,
            ))
            .await?
            .event_id())
    }

    async fn add_member(
        &self,
        journal_id: JournalId,
        member_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
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

    async fn update_member_permissions(
        &self,
        journal_id: JournalId,
        member_id: UserId,
        permissions: Permissions,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
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

    async fn remove_member(
        &self,
        journal_id: JournalId,
        member_id: UserId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        Ok(self
            .decision_maker
            .make(RemoveJournalMember::new(
                journal_id, member_id, authority, timestamp,
            ))
            .await?
            .event_id())
    }

    async fn get_journal(
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

        let journal = sqlx::query_as!(
            JournalStateWithPayload,
            r#"
            SELECT j.id as "id: JournalId", j.owner_id as "owner_id: UserId", j.name as "name: Name", e.payload as "payload!: JournalDomainEvent"
            FROM journals j
            INNER JOIN event e
                ON e.journal_id = $1 AND e.event_type = 'JournalCreated'
            WHERE j.id = $1
            "#,
            journal_id as JournalId)
            .fetch_optional(&self.projection_pool)
            .await?;

        if let Some(journal) = journal {
            match journal.payload {
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
            Err(InvalidJournal(journal_id))
        }
    }

    async fn list_accessible_journals(
        &self,
        user: UserId,
    ) -> JournalResult<Vec<(JournalState, Authority, Timestamp)>> {
        // NOTE(gabriel): a user must not be both a member and the owner, or this query will return duplicate journals

        let journals = sqlx::query_as!(
            JournalStateWithPayload,
            r#"
            SELECT j.id as "id: JournalId", j.owner_id as "owner_id: UserId", j.name as "name: Name", e.payload as "payload!: JournalDomainEvent"
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
            match journal.payload {
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

    async fn create_account(
        &self,
        account_id: AccountId,
        journal_id: JournalId,
        name: Name,
        account_type: AccountType,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
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

    async fn list_journal_accounts(
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

        let accounts = sqlx::query_as!(
            AccountStateWithPayload,
            r#"
            SELECT a.id as "id: AccountId", a.journal_id as "journal_id: JournalId", a.balance, a.name as "name: Name", a.account_type as "account_type: AccountType", e.payload as "payload!: JournalDomainEvent"
            FROM accounts a
            INNER JOIN event e
                ON e.account_id = a.id AND e.event_type = 'AccountCreated'
            WHERE a.journal_id = $1
            "#,
            journal_id as JournalId)
            .fetch_all(self.pool())
            .await?;

        let mut accounts_with_meta = Vec::with_capacity(accounts.len());

        for account in accounts {
            match account.payload {
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

    async fn create_fund(
        &self,
        fund_id: FundId,
        journal_id: JournalId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        Ok(self
            .decision_maker
            .make(CreateFund::new(
                fund_id, journal_id, name, authority, timestamp,
            ))
            .await?
            .event_id())
    }

    async fn list_journal_funds(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(FundState, Authority, Timestamp)>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(InvalidJournal(journal_id));
        }

        let funds = sqlx::query_as!(
            FundStateWithPayload,
            r#"
            SELECT f.id as "id: FundId", f.journal_id as "journal_id: JournalId", f.name as "name: Name", f.balance, e.payload as "payload!: JournalDomainEvent"
            FROM funds f
            INNER JOIN event e
                ON e.account_id = f.id AND e.event_type = 'FundCreated'
            WHERE e.journal_id = $1
            "#,
            journal_id as JournalId)
            .fetch_all(self.pool())
            .await?;

        let mut funds_with_meta = Vec::with_capacity(funds.len());

        for fund in funds {
            match fund.payload {
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

    async fn create_activity(
        &self,
        activity_id: ActivityId,
        journal_id: JournalId,
        name: Name,
        activity_kind: ActivityKind,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        Ok(self
            .decision_maker
            .make(CreateActivity::new(
                activity_id,
                journal_id,
                name,
                activity_kind,
                authority,
                timestamp,
            ))
            .await?
            .event_id())
    }

    async fn list_journal_activities(
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
            SELECT a.id as "id: ActivityId", a.journal_id as "journal_id: JournalId", a.name as "name: Name", a.activity_kind as "activity_kind: ActivityKind", a.balance, e.payload as "payload!: JournalDomainEvent"
            FROM activities a
            INNER JOIN event e
                ON e.account_id = a.id AND e.event_type = 'ActivityCreated'
            WHERE e.journal_id = $1
            "#,
            journal_id as JournalId)
            .fetch_all(self.pool())
            .await?;

        let mut activities_with_meta = Vec::with_capacity(activities.len());

        for activity in activities {
            match activity.payload {
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
                            activity_type: activity.activity_kind,
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

    async fn create_transaction(
        &self,
        transaction_id: TransactionId,
        journal_id: JournalId,
        entries: Vec<TransactionEntry>,
        period: FinancialPeriod,
        memo: Option<Memo>,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        Ok(self
            .decision_maker
            .make(CreateTransaction::new(
                transaction_id,
                journal_id,
                TransactionEntries(entries),
                period,
                memo,
                authority,
                timestamp,
            ))
            .await?
            .event_id())
    }

    async fn list_journal_transactions(
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
            SELECT t.id as "id: TransactionId", t.journal_id as "journal_id: JournalId", t.memo as "memo: Memo", ev.payload as "payload!: JournalDomainEvent",
                   ARRAY(
                        SELECT (en.id, en.amount, en.entry_side, en.entry_kind)
                        FROM entries en
                        WHERE en.transaction_id = t.id AND t.journal_id = $1
                   ) AS "entries!: Vec<(EntryId, i64, EntrySide, EntryKind)>"
            FROM transactions t
            INNER JOIN event ev
                ON ev.transaction_id = t.id AND ev.event_type = 'TransactionCreated'
            WHERE t.journal_id = $1
            "#,
            journal_id as JournalId)
            .fetch_all(self.pool())
            .await?;

        let mut transactions_with_meta = Vec::with_capacity(transactions.len());

        for transaction in transactions {
            let entries = transaction
                .entries
                .into_iter()
                .map(
                    |(id, amount, entry_side, entry_kind)| StoredTransactionEntry {
                        id,
                        amount,
                        entry_side,
                        entry_kind,
                    },
                )
                .collect();

            match transaction.payload {
                JournalDomainEvent::TransactionCreated {
                    authority,
                    timestamp,
                    ..
                } => {
                    transactions_with_meta.push((
                        TransactionState {
                            id: transaction.id,
                            journal_id,
                            memo: transaction.memo,
                            entries,
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

    async fn upload_file(
        &self,
        file_id: FileId,
        journal_id: JournalId,
        hash: [u8; 16],
        file_name: String,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        Ok(self
            .decision_maker
            .make(UploadFile::new(
                file_id, journal_id, hash, file_name, authority, timestamp,
            ))
            .await?
            .event_id())
    }

    async fn list_journal_files(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(FileState, Authority, Timestamp)>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(InvalidJournal(journal_id));
        }

        let files = sqlx::query_as!(
            FileStateWithPayload,
            r#"
            SELECT f.id as "id: FileId", f.journal_id as "journal_id: JournalId", f.hash as "hash: [u8; 16]", f.name, e.payload as "payload!: JournalDomainEvent"
            FROM files f
            INNER JOIN event e
                ON e.file_id = f.id AND e.event_type = 'FileUploaded'
            WHERE f.journal_id = $1
            "#,
            journal_id as JournalId
        )
            .fetch_all(self.pool())
            .await?;

        let mut files_with_meta = Vec::with_capacity(files.len());

        for file in files {
            match file.payload {
                JournalDomainEvent::FileUploaded {
                    authority,
                    timestamp,
                    ..
                } => files_with_meta.push((
                    FileState {
                        id: file.id,
                        journal_id,
                        hash: file.hash,
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

    async fn delete_file(
        &self,
        file_id: FileId,
        journal_id: JournalId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        Ok(self
            .decision_maker
            .make(DeleteFile::new(file_id, journal_id, authority, timestamp))
            .await?
            .event_id())
    }

    async fn import_jewel_database(
        &self,
        journal_id: JournalId,
        jewel_data: JewelData,
        authority: Authority,
    ) -> JournalResult<EventId> {
        let mut creation_timestamp = Utc::now();

        let mut jewel_account_id_map = HashMap::new();
        let mut latest_event_id = 0;

        for (jewel_account_id, account) in jewel_data.accounts {
            if account.active {
                let account_type = if account.account_type < 3 {
                    AccountType::Asset
                } else {
                    AccountType::Liability
                };

                // jewel doesn't record account creation timestamps, but its ids are sequential
                // since we're creating the accounts in that sequential order, we can keep the order
                // by creating each account 1 millisecond later than the last
                creation_timestamp += Duration::from_millis(1);

                // looping pattern to retry in the case of an id collision
                let mut account_id = AccountId::new();
                loop {
                    match self
                        .create_account(
                            account_id,
                            journal_id,
                            // jewel names appear to be max 50 characters, ours are max 64
                            Name::try_new(account.name.clone())?,
                            account_type,
                            authority,
                            creation_timestamp.get_time(),
                        )
                        .await
                    {
                        Ok(event_id) => {
                            latest_event_id = event_id;
                            break;
                        }
                        Err(JournalError::AccountIdCollision(_)) => account_id = AccountId::new(),
                        Err(e) => Err(e)?,
                    };
                }
                jewel_account_id_map.insert(jewel_account_id, account_id);
            }
        }

        for (jewel_transaction_id, jewel_transaction) in jewel_data.journals {
            let z_single_account_id = jewel_transaction.z_single_account_id;

            let mut entries = Vec::new();

            let mut entry_sum = 0;

            for entry in jewel_data
                .journal_entries
                .get(&jewel_transaction_id)
                .ok_or(JewelImportError::MissingJournalEntries(
                    jewel_transaction_id,
                ))
                .map_err(JournalError::JewelImport)?
            {
                if entry.account_id == jewel_transaction.z_single_account_id {
                    continue;
                }

                let entry_amount = (entry.amount * 100.0).round() as i64;

                entry_sum += entry_amount;

                entries.push(TransactionEntry {
                    amount: entry_amount.unsigned_abs(),
                    entry_side: if entry_amount > 0 {
                        EntrySide::Debit
                    } else {
                        EntrySide::Credit
                    },
                    entry_kind: EntryKind::Account {
                        account_id: *jewel_account_id_map
                            .get(&entry.account_id)
                            .ok_or(JewelImportError::MissingAccount(entry.account_id))
                            .map_err(JournalError::JewelImport)?,
                    },
                })
            }

            entries.push(TransactionEntry {
                amount: entry_sum.unsigned_abs(),
                entry_side: if entry_sum < 0 {
                    EntrySide::Debit
                } else {
                    EntrySide::Credit
                },
                entry_kind: EntryKind::Account {
                    account_id: *jewel_account_id_map
                        .get(&z_single_account_id)
                        .ok_or(JewelImportError::MissingAccount(z_single_account_id))
                        .map_err(JournalError::JewelImport)?,
                },
            });

            let mut transaction_id = TransactionId::new();

            loop {
                match self
                    .create_transaction(
                        transaction_id,
                        journal_id,
                        // cloning feels really silly here, but making this borrowed is actually quite challenging
                        // the performance impact should be negligible because the list should be small and
                        // the chances of an id collision are mathematically insignificant
                        entries.clone(),
                        FinancialPeriod::from(jewel_transaction.accounting_date),
                        // jewel memos appear to be max 50 characters, ours are max 100 characters
                        Some(
                            Memo::try_new(jewel_transaction.memo.clone())
                                .map_err(JournalError::Memo)?,
                        ),
                        authority,
                        jewel_transaction.accounting_date.get_time(),
                    )
                    .await
                {
                    Ok(event_id) => {
                        latest_event_id = event_id;
                        break;
                    }
                    Err(JournalError::TransactionIdCollision(_)) => {
                        transaction_id = TransactionId::new()
                    }
                    Err(e) => Err(e)?,
                };
            }
        }
        Ok(latest_event_id)
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
                .execute(&mut **tx)
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
            ..
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
                .execute(&mut **tx)
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
            .execute(&mut **tx)
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
            .execute(&mut **tx)
            .await?;
        }
        JournalDomainEvent::TransactionCreated {
            transaction_id,
            journal_id,
            memo,
            ..
        } => {
            sqlx::query!(
                    r#"
                    INSERT INTO transactions (id, journal_id, memo) VALUES($1, $2, $3) ON CONFLICT DO NOTHING
                    "#,
                    transaction_id as TransactionId,
                    journal_id as JournalId,
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
                .execute(&mut **tx)
                .await?;
        }
        JournalDomainEvent::FileDeleted {
            file_id,
            journal_id,
            ..
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
            ..
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
            ..
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
    }

    Ok(())
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

        let mut tx = self.projection_pool.begin().await?;
        handle_incoming_event(event.into_inner(), &mut tx).await?;
        tx.commit().await?;

        self.current_event
            .send(event_id)
            .expect("journal eventid sender closed");

        Ok(())
    }
}
