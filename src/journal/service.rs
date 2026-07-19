use crate::authn::AuthConnectError;
use crate::authn::user::UserId;
use crate::authority::{Actor, Authority};
use crate::journal::JournalError;
use crate::journal::JournalError::PermissionError;
use crate::journal::JournalId;
use crate::journal::JournalResult;
use crate::journal::PermissionDecodeError;
use crate::journal::Permissions;
use crate::journal::account::AccountId;
use crate::journal::domain::JournalDomainEvent;
use crate::journal::store::JournalEventStore;
use crate::journal::transaction::{BalanceUpdate, EntryType, TransactionId};
use crate::msgpack::MsgPack;
use crate::name::Name;
use crate::time_provider::Timestamp;
use async_trait::async_trait;
use disintegrate::serde::messagepack::MessagePack;
use disintegrate::{EventListener, PersistedEvent, StreamQuery, query};
use disintegrate_postgres::{
    PgDecisionMaker, PgEventId, PgSnapshotter, WithPgSnapshot, decision_maker,
};
use sqlx::{FromRow, PgPool};
use tokio::sync::watch;

type PgJournalDecisionMaker =
    PgDecisionMaker<JournalDomainEvent, MessagePack<JournalDomainEvent>, WithPgSnapshot>;

pub struct JournalState {
    pub id: JournalId,
    pub owner_id: UserId,
    pub name: Name,
}

pub struct AccountState {
    pub id: AccountId,
    #[expect(unused)]
    pub journal_id: JournalId,
    pub name: Name,
    pub balance: i64,
}

pub struct TransactionState {
    pub id: TransactionId,
    #[expect(unused)]
    pub journal_id: JournalId,
    pub entries: Vec<BalanceUpdate>,
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
    journal_id: JournalId,
    name: Name,
    balance: i64,
    payload: Vec<u8>,
}
#[derive(FromRow)]
struct TransactionStateWithPayload {
    id: TransactionId,
    journal_id: JournalId,
    updates: MsgPack<Vec<BalanceUpdate>>,
    payload: Vec<u8>,
}

#[derive(Clone)]
pub struct JournalService {
    query: StreamQuery<PgEventId, JournalDomainEvent>,
    projection_pool: PgPool,
    pub decision_maker: PgJournalDecisionMaker,
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
                updates BYTEA NOT NULL
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

    pub async fn get_effective_permissions(
        &self,
        journal_id: JournalId,
        authority: &Authority,
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
            let payload: JournalDomainEvent = rmp_serde::from_slice(journal.payload.as_slice())?;

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
        authority: &Authority,
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
            let payload: JournalDomainEvent = rmp_serde::from_slice(journal.payload.as_slice())?;

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
        authority: &Authority,
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
        authority: &Authority,
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
            SELECT a.id as "id: AccountId", a.journal_id as "journal_id: JournalId", a.balance, a.name as "name: Name", e.payload as "payload!"
            FROM accounts a
            INNER JOIN event e
                ON e.account_id = a.id AND e.event_type = 'AccountCreated'
            WHERE a.journal_id = $1
            "#,
            journal_id as JournalId)
            .fetch_all(&self.projection_pool)
            .await?;

        let mut transactions_with_meta = Vec::with_capacity(accounts.len());

        for account in accounts {
            let payload: JournalDomainEvent = rmp_serde::from_slice(account.payload.as_slice())?;

            match payload {
                JournalDomainEvent::AccountCreated {
                    authority,
                    timestamp,
                    ..
                } => {
                    transactions_with_meta.push((
                        AccountState {
                            id: account.id,
                            journal_id: account.journal_id,
                            name: account.name,
                            balance: account.balance,
                        },
                        authority,
                        timestamp,
                    ));
                }
                _ => unreachable!("AccountCreated events are filtered by the sql query"),
            }
        }

        Ok(transactions_with_meta)
    }

    pub async fn list_journal_transactions(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> JournalResult<Vec<(TransactionState, Authority, Timestamp)>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(PermissionError(Permissions::READ));
        }

        let transactions = sqlx::query_as!(
            TransactionStateWithPayload,
            r#"
            SELECT t.id as "id: TransactionId", t.journal_id as "journal_id: JournalId", t.updates as "updates: MsgPack<Vec<BalanceUpdate>>", e.payload as "payload!"
            FROM transactions t
            INNER JOIN event e
                ON e.transaction_id = t.id AND e.event_type = 'TransactionCreated'
            WHERE t.journal_id = $1
            "#,
            journal_id as JournalId)
            .fetch_all(&self.projection_pool)
            .await?;

        let mut transactions_with_meta = Vec::with_capacity(transactions.len());

        for transaction in transactions {
            let payload: JournalDomainEvent =
                rmp_serde::from_slice(transaction.payload.as_slice())?;

            match payload {
                JournalDomainEvent::TransactionCreated {
                    authority,
                    timestamp,
                    ..
                } => {
                    transactions_with_meta.push((
                        TransactionState {
                            id: transaction.id,
                            journal_id: transaction.journal_id,
                            entries: transaction.updates.0,
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
                ..
            } => {
                sqlx::query!(
                    r#"
                    INSERT INTO accounts (id, journal_id, name, balance) VALUES($1, $2, $3, 0) ON CONFLICT DO NOTHING
                    "#,
                    account_id as AccountId,
                    journal_id as JournalId,
                    name as Name
                )
                .execute(&self.projection_pool)
                .await?;
            }
            JournalDomainEvent::AccountRenamed {
                account_id,
                new_name,
                ..
            } => {
                sqlx::query!(
                    r#"
                    UPDATE accounts SET name = $1 WHERE id = $2
                    "#,
                    new_name as Name,
                    account_id as AccountId,
                )
                .execute(&self.projection_pool)
                .await?;
            }
            JournalDomainEvent::AccountDeleted { account_id, .. } => {
                sqlx::query!(
                    r#"
                    DELETE FROM accounts WHERE id = $1
                    "#,
                    account_id as AccountId,
                )
                .execute(&self.projection_pool)
                .await?;
            }
            JournalDomainEvent::TransactionCreated {
                transaction_id,
                journal_id,
                balance_updates,
                ..
            } => {
                let mut tx = self.projection_pool.begin().await?;

                sqlx::query!(
                    r#"
                    INSERT INTO transactions (id, journal_id, updates) VALUES($1, $2, $3) ON CONFLICT DO NOTHING
                    "#,
                    transaction_id as TransactionId,
                    journal_id as JournalId,
                    MsgPack(balance_updates.clone()) as MsgPack<Vec<BalanceUpdate>>
                )
                .execute(&mut *tx)
                .await?;

                // apply the balance updates to each account
                for update in balance_updates {
                    let update_amt = match update.entry_type {
                        EntryType::Credit => update.amount as i64,
                        EntryType::Debit => -(update.amount as i64),
                    };

                    sqlx::query!(
                        r#"
                        UPDATE accounts SET balance = balance + $1 WHERE id = $2
                        "#,
                        update_amt,
                        update.account_id as AccountId
                    )
                    .execute(&mut *tx)
                    .await?;
                }

                tx.commit().await?;
            }
            JournalDomainEvent::TransactionDeleted { transaction_id, .. } => {
                let mut tx = self.projection_pool.begin().await?;

                let balance_updates = sqlx::query_scalar!(
                    r#"
                    DELETE FROM transactions WHERE id = $1 RETURNING updates as "updates: MsgPack<Vec<BalanceUpdate>>"
                    "#,
                    transaction_id as TransactionId,
                    )
                    .fetch_one(&mut *tx)
                    .await?;

                // revert the transaction's balance updates
                for update in balance_updates.0 {
                    let update_amt = match update.entry_type {
                        EntryType::Credit => update.amount as i64,
                        EntryType::Debit => -(update.amount as i64),
                    };

                    sqlx::query!(
                        r#"
                        UPDATE accounts SET balance = balance - $1 WHERE id = $2
                        "#,
                        update_amt,
                        update.account_id as AccountId
                    )
                    .execute(&mut *tx)
                    .await?;
                }
                tx.commit().await?;
            }
        }

        self.current_event
            .send(event_id)
            .expect("journal eventid sender closed");

        Ok(())
    }
}
