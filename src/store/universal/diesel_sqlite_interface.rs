use crate::account::{AccountEntity, AccountId, AccountPayload, AccountState};
use crate::auth::user::{UserEntity, UserPayload, UserState};
use crate::authority::{Actor, Authority, UserId};
use crate::email::Email;
use crate::journal::{
    JournalEntity, JournalId, JournalModifiedPayload, JournalPayload, JournalState, Permissions,
};
use crate::name::Name;
use crate::schema::{accounts, journal_members, journals, transactions, users};
use crate::store::universal::diesel_sqlite::DieselSqliteStore;
use crate::store::universal::error::StoreError::AccountNotInJournal;
use crate::store::universal::error::{StoreError, StoreResult};
use crate::store::universal::interface::account::AccountInterface;
use crate::store::universal::interface::auth::AuthInterface;
use crate::store::universal::interface::auth::DEV_USERS;
use crate::store::universal::interface::journal::JournalInterface;
use crate::store::universal::interface::transaction::TransactionInterface;
use crate::store::universal::time_provider::DefaultTimeProvider;
use crate::store::universal::{After, When};
use crate::store::universal::{EventId, Store};
use crate::transaction::{
    BalanceUpdate, TransactionEntity, TransactionId, TransactionPayload, TransactionState,
};
use axum_login::AuthnBackend;
use diesel::sql_types::Binary;
use diesel::{BoolExpressionMethods, NullableExpressionMethods, QueryableByName, sql_query};
use diesel::{ExpressionMethods, JoinOnDsl};
use diesel::{OptionalExtension, QueryDsl, RunQueryDsl};
use std::collections::HashSet;
use webauthn_rs::prelude::Uuid;

#[derive(Clone)]
pub struct DieselSqliteAccountInterface {
    pub store: DieselSqliteStore,
    pub journal_interface: DieselSqliteJournalInterface,
    pub time_provider: &'static DefaultTimeProvider,
}

impl DieselSqliteAccountInterface {
    pub fn new(
        store: DieselSqliteStore,
        journal_interface: DieselSqliteJournalInterface,
        time_provider: &'static DefaultTimeProvider,
    ) -> Self {
        Self {
            store,
            journal_interface,
            time_provider,
        }
    }
}

impl AccountInterface for DieselSqliteAccountInterface {
    async fn create_account(
        &self,
        journal_id: JournalId,
        name: Name,
        authority: &Authority,
    ) -> StoreResult<AccountId> {
        let account_id = AccountId::new();

        self.journal_interface
            .validate_permissions(journal_id, authority, Permissions::ADD_ACCOUNT)
            .await?;

        self.store
            .record::<AccountEntity, _>(
                authority,
                self.time_provider,
                account_id,
                AccountPayload::Created {
                    journal_id,
                    parent_account_id: None,
                    name,
                },
                When::Empty,
            )
            .await?;

        Ok(account_id)
    }

    async fn create_subaccount(
        &self,
        parent_account_id: AccountId,
        journal_id: JournalId,
        name: Name,
        authority: &Authority,
    ) -> StoreResult<AccountId> {
        let account_id = AccountId::new();

        self.store
            .record::<AccountEntity, _>(
                authority,
                self.time_provider,
                account_id,
                AccountPayload::Created {
                    journal_id,
                    parent_account_id: Some(parent_account_id),
                    name,
                },
                When::Empty,
            )
            .await?;

        Ok(account_id)
    }

    async fn get_account(&self, account_id: AccountId) -> StoreResult<AccountState> {
        self.store.get_state::<AccountEntity>(account_id).await
    }

    async fn get_accounts_in_journal(&self, journal_id: JournalId) -> StoreResult<Vec<AccountId>> {
        let conn = self.store.pool.get().await?;

        Ok(conn
            .interact(move |conn| {
                accounts::table
                    .filter(accounts::journal_id.eq(journal_id))
                    .select(accounts::id)
                    .load(conn)
            })
            .await??)
    }
}

#[derive(Clone)]
pub struct DieselSqliteAuthInterface {
    pub store: DieselSqliteStore,
    pub time_provider: &'static DefaultTimeProvider,
}

impl DieselSqliteAuthInterface {
    pub async fn new(
        store: DieselSqliteStore,
        time_provider: &'static DefaultTimeProvider,
    ) -> Self {
        let interface = Self {
            store,
            time_provider,
        };

        for (email, (user_id, webauthn_uuid)) in DEV_USERS.clone() {
            if interface
                .get_id_from_email(email.clone())
                .await
                .expect("failed to check email existence")
                .is_none()
            {
                interface
                    .create_user_with_id(
                        user_id,
                        email,
                        webauthn_uuid,
                        &Authority::Direct(Actor::System),
                    )
                    .await
                    .expect("failed to create dev user");
            }
        }

        interface
    }
}

impl AuthnBackend for DieselSqliteAuthInterface {
    type User = UserState;
    type Credentials = ();
    type Error = StoreError;

    async fn authenticate(
        &self,
        _creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        // Webauthn authentication is handled separately via challenge/response
        // This method is not used - we call session.login() directly after webauthn verification
        Ok(None)
    }

    async fn get_user(&self, user_id: &UserId) -> Result<Option<Self::User>, Self::Error> {
        Ok(Some(AuthInterface::get_state(self, *user_id).await?))
    }
}

impl AuthInterface for DieselSqliteAuthInterface {
    async fn create_user_with_id(
        &self,
        user_id: UserId,
        email: Email,
        webauthn_uuid: Uuid,
        authority: &Authority,
    ) -> StoreResult<()> {
        self.store
            .record::<UserEntity, _>(
                authority,
                self.time_provider,
                user_id,
                UserPayload::Created {
                    email,
                    webauthn_uuid,
                },
                When::Empty,
            )
            .await
            .map(drop)
    }

    async fn get_state(&self, user_id: UserId) -> StoreResult<UserState> {
        self.store.get_state::<UserEntity>(user_id).await
    }

    async fn get_id_from_email(&self, email: Email) -> StoreResult<Option<UserId>> {
        let conn = self.store.pool.get().await?;

        Ok(conn
            .interact(move |conn| {
                users::table
                    .filter(users::email.eq(email))
                    .select(users::id)
                    .first::<UserId>(conn)
                    .optional()
            })
            .await??)
    }

    async fn get_dev_users(&self) -> StoreResult<Vec<UserState>> {
        let conn = self.store.pool.get().await?;

        Ok(conn
            .interact(move |conn| {
                users::table
                    .filter(
                        users::email.eq_any(DEV_USERS.clone().iter().map(|(email, (_, _))| email)),
                    )
                    .load::<UserState>(conn)
            })
            .await??)
    }
}

#[derive(Clone)]
pub struct DieselSqliteJournalInterface {
    pub store: DieselSqliteStore,
    pub time_provider: &'static DefaultTimeProvider,
}

impl DieselSqliteJournalInterface {
    pub fn new(store: DieselSqliteStore, time_provider: &'static DefaultTimeProvider) -> Self {
        Self {
            store,
            time_provider,
        }
    }
}

#[derive(QueryableByName)]
struct JournalIdRow {
    #[diesel(sql_type = Binary)]
    id: JournalId,
}

impl JournalInterface for DieselSqliteJournalInterface {
    async fn create_journal(
        &self,
        name: Name,
        owner: UserId,
        authority: &Authority,
    ) -> StoreResult<JournalId> {
        let journal_id = JournalId::new();

        self.store
            .record::<JournalEntity, _>(
                authority,
                self.time_provider,
                journal_id,
                JournalPayload::Created {
                    name,
                    owner,
                    parent_journal_id: None,
                },
                When::Empty,
            )
            .await?;

        Ok(journal_id)
    }

    async fn create_subjournal(
        &self,
        parent_journal_id: JournalId,
        name: Name,
        authority: &Authority,
    ) -> StoreResult<JournalId> {
        let journal_id = JournalId::new();

        let parent_owner = self
            .store
            .get_state::<JournalEntity>(parent_journal_id)
            .await?
            .owner;

        self.validate_permissions(parent_journal_id, authority, Permissions::CREATE_SUBJOURNAL)
            .await?;

        self.store
            .record::<JournalEntity, _>(
                authority,
                self.time_provider,
                journal_id,
                JournalPayload::Created {
                    name,
                    owner: parent_owner,
                    parent_journal_id: Some(parent_journal_id),
                },
                When::Empty,
            )
            .await?;

        Ok(journal_id)
    }

    async fn get_ancestor_ids(&self, journal_id: JournalId) -> StoreResult<Vec<JournalId>> {
        let conn = self.store.pool.get().await?;

        Ok(conn
            .interact(move |conn| {
                sql_query(
                    r#"
            WITH RECURSIVE journal_tree AS (
                SELECT id, parent_journal_id
                FROM journals
                WHERE id = ?

                UNION ALL

                SELECT j.id, j.parent_journal_id
                FROM journals j
                INNER JOIN journal_tree jt ON j.id = jt.parent_journal_id
            )
            SELECT id FROM journal_tree;
        "#,
                )
                .bind::<Binary, _>(*journal_id)
                .load::<JournalIdRow>(conn)
            })
            .await??
            .iter()
            .map(|row| row.id)
            .collect())
    }

    async fn get_effective_permissions(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> StoreResult<Permissions> {
        let conn = self.store.pool.get().await?;

        Ok(match authority {
            Authority::Direct(actor) => match actor {
                // even if the Permissions are already known, we still need to verify that the journal
                // exists because callers may expect an EntityDoesntExist error if it doesn't
                Actor::Anonymous => {
                    conn.interact(move |conn| {
                        journals::table
                            .filter(journals::id.eq(journal_id))
                            .select(journals::as_of)
                            .first::<EventId>(conn)
                            .optional()
                            .map(|opt_p| opt_p.ok_or(StoreError::EntityNotFound))?
                    })
                    .await??;
                    Permissions::empty()
                }
                Actor::System => {
                    conn.interact(move |conn| {
                        journals::table
                            .filter(journals::id.eq(journal_id))
                            .select(journals::as_of)
                            .first::<EventId>(conn)
                            .optional()
                            .map(|opt_p| opt_p.ok_or(StoreError::EntityNotFound))?
                    })
                    .await??;
                    Permissions::all()
                }
                Actor::User(user_id) => {
                    let user_id = *user_id;

                    conn.interact(move |conn| {
                        journal_members::table
                            .filter(
                                journal_members::journal_id
                                    .eq(journal_id)
                                    .and(journal_members::user_id.eq(user_id)),
                            )
                            .select(journal_members::permissions)
                            .first::<Permissions>(conn)
                            .optional()
                            // the user should not know the entity exists if it isn't accessible to them
                            .map(|opt_p| opt_p.ok_or(StoreError::EntityNotFound))?
                    })
                    .await??
                }
            },
            Authority::Delegated { .. } => todo!(),
        })
    }

    async fn list_accessible_top_level_journals(
        &self,
        user: UserId,
    ) -> StoreResult<Vec<JournalState>> {
        let conn = self.store.pool.get().await?;

        Ok(conn
            .interact(move |conn| {
                journals::table
                    .filter(
                        journals::id.eq_any(
                            journal_members::table
                                .filter(
                                    journal_members::user_id
                                        .eq(user)
                                        .and(journal_members::permissions.ge(Permissions::READ)),
                                )
                                .select(journal_members::journal_id),
                        ),
                    )
                    .load::<JournalState>(conn)
            })
            .await??)
    }

    async fn get_journal(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> StoreResult<JournalState> {
        match authority {
            Authority::Direct(actor) => match actor {
                Actor::User(user_id) => {
                    let conn = self.store.pool.get().await?;

                    let user_id = *user_id;

                    Ok(conn
                        .interact(move |conn| {
                            journals::table
                                .inner_join(
                                    journal_members::table
                                        .on(journal_members::journal_id.eq(journals::id)),
                                )
                                .filter(journals::id.eq(journal_id))
                                .filter(journal_members::user_id.eq(user_id))
                                .filter(journal_members::permissions.ge(Permissions::READ))
                                .select(journals::all_columns)
                                .first::<JournalState>(conn)
                        })
                        .await??)
                }
                Actor::System => {
                    let conn = self.store.pool.get().await?;

                    Ok(conn
                        .interact(move |conn| {
                            journals::table
                                .filter(journals::id.eq(journal_id))
                                .first::<JournalState>(conn)
                        })
                        .await??)
                }
                Actor::Anonymous => Err(StoreError::Permission(Permissions::READ)),
            },
            Authority::Delegated { .. } => todo!(),
        }
    }

    async fn get_direct_subjournals(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> StoreResult<Vec<JournalId>> {
        match authority {
            Authority::Direct(actor) => match actor {
                Actor::User(user_id) => {
                    let conn = self.store.pool.get().await?;

                    let user_id = *user_id;

                    Ok(conn
                        .interact(move |conn| {
                            journals::table
                                .inner_join(
                                    journal_members::table
                                        .on(journal_members::journal_id.eq(journals::id)),
                                )
                                .filter(journals::parent_journal_id.eq(journal_id))
                                .filter(journal_members::user_id.eq(user_id))
                                .filter(journal_members::permissions.ge(Permissions::READ))
                                .select(journals::id)
                                .load(conn)
                        })
                        .await??)
                }
                Actor::System => {
                    let conn = self.store.pool.get().await?;

                    Ok(conn
                        .interact(move |conn| {
                            journals::table
                                .filter(journals::parent_journal_id.eq(journal_id))
                                .select(journals::id)
                                .load(conn)
                        })
                        .await??)
                }
                Actor::Anonymous => Err(StoreError::Permission(Permissions::READ)),
            },
            Authority::Delegated { .. } => todo!(),
        }
    }

    async fn get_descendants(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> StoreResult<Vec<JournalState>> {
        let conn = self.store.pool.get().await?;

        // TODO: integrate the authority check into the main query

        self.validate_permissions(journal_id, authority, Permissions::READ)
            .await?;

        Ok(conn
            .interact(move |conn| {
                sql_query(
                    r#"
        WITH RECURSIVE journal_tree AS (
            SELECT id, name, owner, parent_journal_id, as_of, 0 AS depth
            FROM journals
            WHERE id = ?

            UNION ALL

            SELECT j.id, j.name, j.owner, j.parent_journal_id, j.as_of, jt.depth + 1
            FROM journals j
            INNER JOIN journal_tree jt ON j.parent_journal_id = jt.id
        )
        SELECT id, name, owner, parent_journal_id, as_of
        FROM journal_tree
        WHERE depth > 0
        ORDER BY depth;
            "#,
                )
                .bind::<Binary, _>(*journal_id)
                .load::<JournalState>(conn)
            })
            .await??)
    }

    async fn invite_member(
        &self,
        journal_id: JournalId,
        invitee: Email,
        permissions: Permissions,
        authority: &Authority,
    ) -> StoreResult<()> {
        // TODO: give the user the option to decline invites and integrate the additional queries

        self.validate_permissions(journal_id, authority, Permissions::CREATE_SUBJOURNAL)
            .await?;

        let conn = self.store.pool.get().await?;

        let cloned_invitee = invitee.clone();

        let invitee_id = conn
            .interact(|conn| {
                users::table
                    .filter(users::email.eq(cloned_invitee))
                    .select(users::id)
                    .first::<UserId>(conn)
            })
            .await??;

        let (as_of, member_rowid) = conn
            .interact(move |conn| {
                journals::table
                    .left_join(
                        journal_members::table.on(journal_members::journal_id
                            .eq(*journal_id)
                            .and(journal_members::user_id.eq(*invitee_id))),
                    )
                    .filter(journals::id.eq(journal_id))
                    .select((journals::as_of, journal_members::rowid.nullable()))
                    .first::<(EventId, Option<i32>)>(conn)
            })
            .await??;

        drop(conn);

        if member_rowid.is_none() {
            self.store
                .record::<JournalEntity, _>(
                    authority,
                    self.time_provider,
                    journal_id,
                    JournalPayload::Modified(JournalModifiedPayload::AddedTenant {
                        id: invitee_id,
                        permissions,
                    }),
                    When::Within(as_of),
                )
                .await
                .map(drop)
        } else {
            Err(StoreError::JournalInviteUserHasAccess(invitee))
        }
    }

    async fn update_member_permissions(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        permissions: Permissions,
        authority: &Authority,
    ) -> StoreResult<()> {
        self.validate_permissions(journal_id, authority, Permissions::OWNER)
            .await?;

        let conn = self.store.pool.get().await?;

        let (as_of, member_rowid) = conn
            .interact(move |conn| {
                journals::table
                    .left_join(
                        journal_members::table.on(journal_members::journal_id
                            .eq(*journal_id)
                            .and(journal_members::user_id.eq(target_user))),
                    )
                    .filter(journals::id.eq(journal_id))
                    .select((journals::as_of, journal_members::rowid.nullable()))
                    .first::<(EventId, Option<i32>)>(conn)
            })
            .await??;

        drop(conn);

        if member_rowid.is_some() {
            self.store
                .record::<JournalEntity, _>(
                    authority,
                    self.time_provider,
                    journal_id,
                    JournalPayload::Modified(JournalModifiedPayload::UpdatedTenantPermissions {
                        id: target_user,
                        permissions,
                    }),
                    When::Within(as_of),
                )
                .await
                .map(drop)
        } else {
            Err(StoreError::JournalModifyNoAccess(target_user))
        }
    }

    async fn remove_member(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        authority: &Authority,
    ) -> StoreResult<()> {
        self.validate_permissions(journal_id, authority, Permissions::OWNER)
            .await?;

        let conn = self.store.pool.get().await?;

        let (as_of, member_rowid) = conn
            .interact(move |conn| {
                journals::table
                    .left_join(
                        journal_members::table.on(journal_members::journal_id
                            .eq(*journal_id)
                            .and(journal_members::user_id.eq(target_user))),
                    )
                    .filter(journals::id.eq(journal_id))
                    .select((journals::as_of, journal_members::rowid.nullable()))
                    .first::<(EventId, Option<i32>)>(conn)
            })
            .await??;

        drop(conn);

        if member_rowid.is_some() {
            self.store
                .record::<JournalEntity, _>(
                    authority,
                    self.time_provider,
                    journal_id,
                    JournalPayload::Modified(JournalModifiedPayload::RemovedTenant {
                        id: target_user,
                    }),
                    When::Within(as_of),
                )
                .await
                .map(drop)
        } else {
            Err(StoreError::JournalModifyNoAccess(target_user))
        }
    }

    async fn get_creator(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> StoreResult<Authority> {
        self.validate_permissions(journal_id, authority, Permissions::READ)
            .await?;

        self.store
            .review::<JournalEntity>(journal_id, After::Start, 1)
            .await?
            .events
            .into_iter()
            .next()
            .map(|e| e.authority.0)
            .ok_or(StoreError::EntityNotFound)
    }
}

#[derive(Clone)]
pub struct DieselSqliteTransactionInterface {
    pub store: DieselSqliteStore,
    pub journal_interface: DieselSqliteJournalInterface,
    pub account_interface: DieselSqliteAccountInterface,
    pub time_provider: &'static DefaultTimeProvider,
}

impl DieselSqliteTransactionInterface {
    pub fn new(
        store: DieselSqliteStore,
        journal_interface: DieselSqliteJournalInterface,
        account_interface: DieselSqliteAccountInterface,
        time_provider: &'static DefaultTimeProvider,
    ) -> Self {
        Self {
            store,
            journal_interface,
            account_interface,
            time_provider,
        }
    }
}

impl TransactionInterface for DieselSqliteTransactionInterface {
    async fn create_transaction(
        &self,
        journal_id: JournalId,
        updates: Vec<BalanceUpdate>,
        authority: &Authority,
    ) -> StoreResult<TransactionId> {
        let transaction_id = TransactionId::new();

        self.journal_interface
            .validate_permissions(journal_id, authority, Permissions::APPEND_TRANSACTION)
            .await?;

        let journal_accounts = self
            .account_interface
            .get_accounts_in_journal(journal_id)
            .await?
            .into_iter()
            .collect::<HashSet<AccountId>>();

        for update in updates.iter() {
            if !journal_accounts.contains(&update.account_id) {
                return Err(AccountNotInJournal {
                    journal_id,
                    account_id: update.account_id,
                });
            }
        }

        self.store
            .record::<TransactionEntity, _>(
                authority,
                self.time_provider,
                transaction_id,
                TransactionPayload::Created {
                    journal_id,
                    updates,
                },
                When::Empty,
            )
            .await?;
        Ok(transaction_id)
    }

    async fn get_all_in_journal(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> StoreResult<Vec<TransactionState>> {
        self.journal_interface
            .validate_permissions(journal_id, authority, Permissions::READ)
            .await?;

        let conn = self.store.pool.get().await?;
        Ok(conn
            .interact(move |conn| {
                transactions::table
                    .filter(transactions::journal_id.eq(journal_id))
                    .load(conn)
            })
            .await??)
    }

    async fn get_creator(
        &self,
        transaction_id: TransactionId,
        authority: &Authority,
    ) -> StoreResult<Authority> {
        let creation_event = self
            .store
            .review::<TransactionEntity>(transaction_id, After::Start, 1)
            .await?
            .events
            .into_iter()
            .next()
            .ok_or(StoreError::EntityNotFound)?;

        match creation_event.payload {
            TransactionPayload::Created { journal_id, .. } => {
                self.journal_interface
                    .validate_permissions(journal_id, authority, Permissions::READ)
                    .await?;
                Ok(creation_event.authority.0)
            }
            TransactionPayload::Modified(_) => unreachable!(
                "the first event pertaining to an entity should always be a creation event"
            ),
        }
    }
}
