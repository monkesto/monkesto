use crate::authn::AuthConnectError;
use crate::authn::user::UserId;
use crate::authority::{Actor, Authority};
use crate::journal::JournalError;
use crate::journal::JournalId;
use crate::journal::JournalResult;
use crate::journal::PermissionDecodeError;
use crate::journal::Permissions;
use crate::journal::account::{AccountId, AccountType};
use crate::journal::activity::ActivityId;
use crate::journal::domain::JournalDomainEvent;
use crate::journal::entry::{EntryId, EntryKind, EntrySide};
use crate::journal::file::{FileId, ObjectStore};
use crate::journal::fund::FundId;
use crate::journal::store::JournalEventStore;
use crate::journal::transaction::{TransactionEntryIds, TransactionId};
use crate::name::Name;
use async_trait::async_trait;
use disintegrate::serde::prost::Prost;
use disintegrate::{EventListener, PersistedEvent, StreamQuery, query};
use disintegrate_postgres::{
    PgDecisionMaker, PgEventId, PgSnapshotter, WithPgSnapshot, decision_maker,
};
use proto::event::journal::ProtoJournalDomainEvent;
use sqlx::PgPool;
use tokio::sync::watch;

type PgJournalDecisionMaker = PgDecisionMaker<
    JournalDomainEvent,
    Prost<JournalDomainEvent, ProtoJournalDomainEvent>,
    WithPgSnapshot,
>;

#[derive(Clone)]
pub struct JournalService {
    query: StreamQuery<PgEventId, JournalDomainEvent>,
    pub(crate) projection_pool: PgPool,
    pub(crate) decision_maker: PgJournalDecisionMaker,
    current_event: watch::Sender<PgEventId>,
    pub(crate) object_store: ObjectStore,
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
            object_store: ObjectStore::new().await,
        })
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
