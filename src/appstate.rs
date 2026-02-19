use crate::auth::MemoryUserStore;
use crate::auth::user::Email;
use crate::auth::user::UserStore;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::authority::UserId;
use crate::event::EventStore;
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::journal::JournalEvent;
use crate::journal::JournalMemoryStore;
use crate::journal::JournalState;
use crate::journal::JournalStore;
use crate::journal::JournalTenantInfo;
use crate::journal::Permissions;
use crate::journal::account::AccountEvent;
use crate::journal::account::AccountMemoryStore;
use crate::journal::account::AccountState;
use crate::journal::account::AccountStore;
use crate::journal::transaction::BalanceUpdate;
use crate::journal::transaction::EntryType;
use crate::journal::transaction::TransactionEvent;
use crate::journal::transaction::TransactionMemoryStore;
use crate::journal::transaction::TransactionState;
use crate::journal::transaction::TransactionStore;
use crate::known_errors::KnownErrors;
use crate::known_errors::KnownErrors::PermissionError;
use chrono::Utc;
use std::str::FromStr;

pub(crate) trait AppState: Sized {
    type UserStore: UserStore;
    type JournalStore: JournalStore;
    type TransactionStore: TransactionStore;
    type AccountStore: AccountStore;

    fn user_store(&self) -> &Self::UserStore;

    fn journal_store(&self) -> &Self::JournalStore;

    fn transaction_store(&self) -> &Self::TransactionStore;

    fn account_store(&self) -> &Self::AccountStore;

    async fn journal_create(
        &self,
        journal_id: JournalId,
        name: String,
        actor: UserId,
    ) -> Result<(), KnownErrors> {
        self.journal_store()
            .record(
                journal_id,
                Authority::Direct(Actor::Anonymous),
                JournalEvent::Created {
                    name,
                    creator: actor,
                    created_at: Utc::now(),
                },
                None,
            )
            .await
    }

    async fn journal_list(
        &self,
        actor: UserId,
    ) -> Result<Vec<(JournalId, JournalState)>, KnownErrors> {
        let ids = self.journal_store().get_user_journals(actor).await?;

        let mut journals = Vec::new();

        for id in ids {
            journals.push((
                id,
                self.journal_store()
                    .get_journal(id)
                    .await?
                    .ok_or(KnownErrors::InvalidJournal)?,
            ))
        }

        Ok(journals)
    }

    async fn journal_get(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Option<JournalState>, KnownErrors> {
        let state = self.journal_store().get_journal(journal_id).await;

        match state {
            Ok(Some(s)) => {
                if s.get_user_permissions(actor).contains(Permissions::READ) {
                    Ok(Some(s))
                } else {
                    Ok(None)
                }
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    async fn journal_get_users(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Vec<UserId>, KnownErrors> {
        let journal_state = self
            .journal_store()
            .get_journal(journal_id)
            .await?
            .ok_or(KnownErrors::InvalidJournal)?;

        if !journal_state
            .get_user_permissions(actor)
            .contains(Permissions::READ)
        {
            return Err(PermissionError {
                required_permissions: Permissions::READ,
            });
        }

        let mut users = Vec::new();

        for (user_id, _) in journal_state.tenants.iter() {
            users.push(*user_id);
        }

        users.push(journal_state.creator);

        Ok(users)
    }

    async fn journal_get_name(&self, journal_id: JournalId) -> Result<Option<String>, KnownErrors> {
        self.journal_store().get_name(journal_id).await
    }

    async fn journal_get_name_from_res<T>(
        &self,
        journal_id_res: Result<JournalId, T>,
    ) -> Result<Option<String>, KnownErrors>
    where
        KnownErrors: From<T>,
    {
        self.journal_get_name(journal_id_res?).await
    }

    async fn journal_invite_tenant(
        &self,
        journal_id: JournalId,
        actor: UserId,
        invitee: Email,
        permissions: Permissions,
    ) -> Result<(), KnownErrors> {
        let journal_state = self
            .journal_store()
            .get_journal(journal_id)
            .await?
            .ok_or(KnownErrors::InvalidJournal)?;

        if journal_state.deleted {
            return Err(KnownErrors::InvalidJournal);
        }

        let invitee_id = self
            .user_store()
            .lookup_user_id(invitee.as_ref())
            .await
            .map_err(|e| KnownErrors::InternalError {
                context: e.to_string(),
            })?
            .ok_or(KnownErrors::UserDoesntExist)?;

        if journal_state.tenants.contains_key(&invitee_id) {
            return Err(KnownErrors::UserCanAccessJournal);
        }

        if !journal_state
            .get_user_permissions(actor)
            .contains(Permissions::INVITE)
        {
            return Err(PermissionError {
                required_permissions: Permissions::INVITE,
            });
        }

        let tenant_info = JournalTenantInfo {
            tenant_permissions: permissions,
            inviting_user: actor,
            invited_at: Utc::now(),
        };

        self.journal_store()
            .record(
                journal_id,
                Authority::Direct(Actor::Anonymous),
                JournalEvent::AddedTenant {
                    id: invitee_id,
                    tenant_info,
                },
                None,
            )
            .await
    }

    async fn journal_update_tenant_permissions(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        permissions: Permissions,
        actor: UserId,
    ) -> Result<(), KnownErrors> {
        let journal_state = self
            .journal_store()
            .get_journal(journal_id)
            .await?
            .ok_or(KnownErrors::InvalidJournal)?;

        if journal_state.deleted {
            return Err(KnownErrors::InvalidJournal);
        }

        if !journal_state.tenants.contains_key(&target_user) {
            return Err(KnownErrors::UserDoesntExist);
        }

        if !journal_state
            .get_user_permissions(actor)
            .contains(Permissions::OWNER)
        {
            return Err(PermissionError {
                required_permissions: Permissions::OWNER,
            });
        }

        self.journal_store()
            .record(
                journal_id,
                Authority::Direct(Actor::Anonymous),
                JournalEvent::UpdatedTenantPermissions {
                    id: target_user,
                    permissions,
                },
                None,
            )
            .await
    }

    async fn journal_remove_tenant(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        actor: UserId,
    ) -> Result<(), KnownErrors> {
        let journal_state = self
            .journal_store()
            .get_journal(journal_id)
            .await?
            .ok_or(KnownErrors::InvalidJournal)?;

        if journal_state.deleted {
            return Err(KnownErrors::InvalidJournal);
        }

        if !journal_state.tenants.contains_key(&target_user) {
            return Err(KnownErrors::UserDoesntExist);
        }

        if !journal_state
            .get_user_permissions(actor)
            .contains(Permissions::OWNER)
        {
            return Err(PermissionError {
                required_permissions: Permissions::OWNER,
            });
        }

        self.journal_store()
            .record(
                journal_id,
                Authority::Direct(Actor::Anonymous),
                JournalEvent::RemovedTenant { id: target_user },
                None,
            )
            .await
    }

    async fn account_create(
        &self,
        account_id: AccountId,
        journal_id: JournalId,
        creator_id: UserId,
        account_name: String,
    ) -> Result<(), KnownErrors> {
        let journal_state = self
            .journal_store()
            .get_journal(journal_id)
            .await?
            .ok_or(KnownErrors::InvalidJournal)?;

        if journal_state.deleted {
            return Err(KnownErrors::InvalidJournal);
        }

        if !journal_state
            .get_user_permissions(creator_id)
            .contains(Permissions::ADDACCOUNT)
        {
            return Err(PermissionError {
                required_permissions: Permissions::ADDACCOUNT,
            });
        }

        if self
            .account_store()
            .get_account(&account_id)
            .await?
            .is_some()
        {
            return Err(KnownErrors::AccountExists);
        }

        self.account_store()
            .record(
                account_id,
                Authority::Direct(Actor::Anonymous),
                AccountEvent::Created {
                    journal_id,
                    name: account_name,
                    creator: creator_id,
                    created_at: Utc::now(),
                },
                None,
            )
            .await?;

        Ok(())
    }

    async fn account_get_all_in_journal(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Vec<(AccountId, AccountState)>, KnownErrors> {
        if !self
            .journal_store()
            .get_permissions(journal_id, actor)
            .await?
            .is_some_and(|p| p.contains(Permissions::READ))
        {
            return Err(PermissionError {
                required_permissions: Permissions::READ,
            });
        }

        let ids = self
            .account_store()
            .get_journal_accounts(journal_id)
            .await?;

        let mut accounts = Vec::new();

        for id in ids {
            accounts.push((
                id,
                self.account_store()
                    .get_account(&id)
                    .await?
                    .ok_or(KnownErrors::AccountDoesntExist { id })?,
            ));
        }

        Ok(accounts)
    }

    async fn account_get_name(&self, account_id: AccountId) -> Result<Option<String>, KnownErrors> {
        self.account_store()
            .get_account(&account_id)
            .await
            .map(|a| a.map(|a| a.name))
    }

    async fn transaction_create(
        &self,
        transaction_id: TransactionId,
        journal_id: JournalId,
        creator_id: UserId,
        updates: Vec<BalanceUpdate>,
    ) -> Result<(), KnownErrors> {
        let journal_accounts = self
            .account_store()
            .get_journal_accounts(journal_id)
            .await?;

        for update in &updates {
            if !journal_accounts.contains(&update.account_id) {
                return Err(KnownErrors::AccountDoesntExist {
                    id: update.account_id,
                });
            }
        }

        if let Some(journal) = self.journal_store().get_journal(journal_id).await?
            && !journal.deleted
        {
            if !journal
                .get_user_permissions(creator_id)
                .contains(Permissions::APPENDTRANSACTION)
            {
                return Err(PermissionError {
                    required_permissions: Permissions::APPENDTRANSACTION,
                });
            }
        } else {
            return Err(KnownErrors::InvalidJournal);
        }

        let event = TransactionEvent::CreatedTransaction {
            journal_id,
            author: creator_id,
            updates,
            created_at: Utc::now(),
        };

        // update the balances first: this will check if the accounts actually exist
        self.account_store().update_balances(&event, None).await?;

        self.transaction_store()
            .record(
                transaction_id,
                Authority::Direct(Actor::Anonymous),
                event,
                None,
            )
            .await?;

        Ok(())
    }

    async fn transaction_get_all_in_journal(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Vec<(TransactionId, TransactionState)>, KnownErrors> {
        if !self
            .journal_store()
            .get_permissions(journal_id, actor)
            .await?
            .ok_or(PermissionError {
                required_permissions: Permissions::READ,
            })?
            .contains(Permissions::READ)
        {
            return Err(PermissionError {
                required_permissions: Permissions::READ,
            });
        }

        let transaction_ids = self
            .transaction_store()
            .get_journal_transactions(journal_id)
            .await?;

        let mut transactions = Vec::new();

        for id in transaction_ids {
            transactions.push((
                id,
                self.transaction_store()
                    .get_transaction(&id)
                    .await?
                    .ok_or(KnownErrors::InvalidTransaction { id })?,
            ));
        }

        Ok(transactions)
    }

    async fn user_get_email(&self, userid: UserId) -> Result<Option<String>, KnownErrors> {
        self.user_store()
            .get_user_email(userid)
            .await
            .map_err(|e| KnownErrors::InternalError {
                context: e.to_string(),
            })
    }

    async fn seed_dev_data(&self) -> Result<(), KnownErrors> {
        // TODO: Unify user seeding

        self.user_store()
            .seed_dev_users()
            .await
            .map_err(|e| KnownErrors::InternalError {
                context: e.to_string(),
            })?;

        let pacioli_id = UserId::from_str("zk8m3p5q7r2n4v6x")?;
        let wedgwood_id = UserId::from_str("yj7l2o4p6q8s0u1w")?;

        let wedgwood_email = Email::try_new(
            self.user_get_email(wedgwood_id)
                .await?
                .ok_or(KnownErrors::UserDoesntExist)?,
        )
        .map_err(|e| KnownErrors::InternalError {
            context: e.to_string(),
        })?;

        let maple_ridge_academy_id = JournalId::from_str("ab1cd2ef3g")?;
        let smith_and_sons_id = JournalId::from_str("hi4jk5lm6n")?;
        let green_valley_id = JournalId::from_str("op7qr8st9u")?;

        let assets_id = AccountId::from_str("ac1assets0")?;
        let revenue_id = AccountId::from_str("ac4revenue")?;
        let expenses_id = AccountId::from_str("ac5expense")?;

        if self
            .journal_get(maple_ridge_academy_id, pacioli_id)
            .await?
            .is_none()
        {
            self.journal_create(
                maple_ridge_academy_id,
                "Maple Ridge Academy".to_owned(),
                pacioli_id,
            )
            .await?;
        }

        if self
            .journal_get(smith_and_sons_id, pacioli_id)
            .await?
            .is_none()
        {
            self.journal_create(
                JournalId::from_str("hi4jk5lm6n")?,
                "Smith & Sons Bakery".to_owned(),
                pacioli_id,
            )
            .await?;
        }

        if self
            .journal_get(green_valley_id, pacioli_id)
            .await?
            .is_none()
        {
            self.journal_create(
                JournalId::from_str("op7qr8st9u")?,
                "Green Valley Farm Co.".to_owned(),
                pacioli_id,
            )
            .await?;
        }

        // journal_get returns none if the actor isn't a tenant
        if self
            .journal_get(maple_ridge_academy_id, wedgwood_id)
            .await?
            .is_none()
        {
            self.journal_invite_tenant(
                maple_ridge_academy_id,
                pacioli_id,
                wedgwood_email,
                Permissions::READ | Permissions::APPENDTRANSACTION,
            )
            .await?;
        }

        // working under the assumption that the presence of any accounts shows that they were already seeded
        // if this is proven to be false, iter().any() is available.
        if self
            .account_get_all_in_journal(maple_ridge_academy_id, pacioli_id)
            .await?
            .is_empty()
        {
            self.account_create(
                assets_id,
                maple_ridge_academy_id,
                pacioli_id,
                "Assets".to_owned(),
            )
            .await?;

            self.account_create(
                AccountId::from_str("ac2liabili")?,
                maple_ridge_academy_id,
                pacioli_id,
                "Liabilities".to_owned(),
            )
            .await?;

            self.account_create(
                AccountId::from_str("ac3equity0")?,
                maple_ridge_academy_id,
                pacioli_id,
                "Equity".to_owned(),
            )
            .await?;

            self.account_create(
                revenue_id,
                maple_ridge_academy_id,
                pacioli_id,
                "Revenue".to_owned(),
            )
            .await?;

            self.account_create(
                expenses_id,
                maple_ridge_academy_id,
                pacioli_id,
                "Expenses".to_owned(),
            )
            .await?;
        }

        // again, the presence of any transactions should show that they were already seeded
        if self
            .transaction_get_all_in_journal(maple_ridge_academy_id, pacioli_id)
            .await?
            .is_empty()
        {
            self.transaction_create(
                TransactionId::from_str("t1tuition0000001")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: assets_id,
                        amount: 500000, // $5,000.00 in cents
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: revenue_id,
                        amount: 500000,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;

            // Transaction 1: Tuition payment received - $5,000
            self.transaction_create(
                TransactionId::from_str("t1tuition0000001")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: assets_id,
                        amount: 500000, // $5,000.00 in cents
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: revenue_id,
                        amount: 500000,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;

            // Transaction 2: Teacher salary payment - $3,200
            self.transaction_create(
                TransactionId::from_str("t2salary00000002")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: expenses_id,
                        amount: 320000, // $3,200.00
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: assets_id,
                        amount: 320000,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;

            // Transaction 3: Textbook purchase - $850
            self.transaction_create(
                TransactionId::from_str("t3textbooks00003")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: expenses_id,
                        amount: 85000, // $850.00
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: assets_id,
                        amount: 85000,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;

            // Transaction 4: Another tuition payment - $4,500
            self.transaction_create(
                TransactionId::from_str("t3textbooks00003")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: assets_id,
                        amount: 450000, // $4,500.00
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: revenue_id,
                        amount: 450000,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;

            // Transaction 5: Supplies purchase - $425
            self.transaction_create(
                TransactionId::from_str("t5supplies000005")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: expenses_id,
                        amount: 42500, // $425.00
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: assets_id,
                        amount: 42500,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;
        }

        Ok(())
    }
}

#[derive(Clone)]
pub struct DefaultAppState<U, J, T, A>
where
    U: UserStore,
    J: JournalStore,
    T: TransactionStore,
    A: AccountStore,
{
    pub(crate) user_store: U,
    pub(crate) journal_store: J,
    pub(crate) transaction_store: T,
    pub(crate) account_store: A,
}

impl<U, J, T, A> DefaultAppState<U, J, T, A>
where
    U: UserStore,
    J: JournalStore,
    T: TransactionStore,
    A: AccountStore,
{
    pub fn new(user_store: U, journal_store: J, transaction_store: T, account_store: A) -> Self {
        Self {
            user_store,
            journal_store,
            transaction_store,
            account_store,
        }
    }
}

pub type MemoryAppState = DefaultAppState<
    MemoryUserStore,
    JournalMemoryStore,
    TransactionMemoryStore,
    AccountMemoryStore,
>;

impl Default for MemoryAppState {
    fn default() -> Self {
        Self::new(
            MemoryUserStore::new(),
            JournalMemoryStore::new(),
            TransactionMemoryStore::new(),
            AccountMemoryStore::new(),
        )
    }
}

impl<U, J, T, A> AppState for DefaultAppState<U, J, T, A>
where
    U: UserStore,
    J: JournalStore,
    T: TransactionStore,
    A: AccountStore,
{
    type UserStore = U;
    type JournalStore = J;
    type TransactionStore = T;
    type AccountStore = A;

    fn user_store(&self) -> &Self::UserStore {
        &self.user_store
    }

    fn journal_store(&self) -> &Self::JournalStore {
        &self.journal_store
    }

    fn transaction_store(&self) -> &Self::TransactionStore {
        &self.transaction_store
    }

    fn account_store(&self) -> &Self::AccountStore {
        &self.account_store
    }
}
