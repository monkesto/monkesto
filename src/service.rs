use crate::account::AccountEvent;
use crate::account::AccountMemoryStore;
use crate::account::AccountState;
use crate::account::AccountStore;
use crate::auth::MemoryUserStore;
use crate::auth::user::Email;
use crate::auth::user::UserStore;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::authority::UserId;
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::journal::JournalEvent;
use crate::journal::JournalMemoryStore;
use crate::journal::JournalState;
use crate::journal::JournalStore;
use crate::journal::JournalTenantInfo;
use crate::journal::Permissions;
use crate::known_errors::KnownErrors;
use crate::known_errors::KnownErrors::PermissionError;
use crate::transaction::BalanceUpdate;
use crate::transaction::TransactionEvent;
use crate::transaction::TransactionMemoryStore;
use crate::transaction::TransactionState;
use crate::transaction::TransactionStore;
use chrono::Utc;

#[derive(Clone)]
pub struct Service<U, J, T, A>
where
    U: UserStore,
    J: JournalStore,
    T: TransactionStore,
    A: AccountStore,
{
    user_store: U,
    journal_store: J,
    transaction_store: T,
    account_store: A,
}

impl<U, J, T, A> Service<U, J, T, A>
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

    pub(crate) fn user_store(&self) -> &U {
        &self.user_store
    }

    pub(crate) async fn journal_create(
        &self,
        journal_id: JournalId,
        name: String,
        actor: UserId,
    ) -> Result<(), KnownErrors> {
        self.journal_store
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

    pub(crate) async fn journal_list(
        &self,
        actor: UserId,
    ) -> Result<Vec<(JournalId, JournalState)>, KnownErrors> {
        let ids = self.journal_store.get_user_journals(actor).await?;

        let mut journals = Vec::new();

        for id in ids {
            journals.push((
                id,
                self.journal_store
                    .get_journal(id)
                    .await?
                    .ok_or(KnownErrors::InvalidJournal)?,
            ))
        }

        Ok(journals)
    }

    pub(crate) async fn journal_get(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Option<JournalState>, KnownErrors> {
        let state = self.journal_store.get_journal(journal_id).await;

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

    pub(crate) async fn journal_get_users(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Vec<UserId>, KnownErrors> {
        let journal_state = self
            .journal_store
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

    pub(crate) async fn journal_get_name(
        &self,
        journal_id: JournalId,
    ) -> Result<Option<String>, KnownErrors> {
        self.journal_store.get_name(journal_id).await
    }

    pub(crate) async fn journal_get_name_from_res<E>(
        &self,
        journal_id_res: Result<JournalId, E>,
    ) -> Result<Option<String>, KnownErrors>
    where
        KnownErrors: From<E>,
    {
        self.journal_get_name(journal_id_res?).await
    }

    pub(crate) async fn journal_invite_tenant(
        &self,
        journal_id: JournalId,
        actor: UserId,
        invitee: Email,
        permissions: Permissions,
    ) -> Result<(), KnownErrors> {
        let journal_state = self
            .journal_store
            .get_journal(journal_id)
            .await?
            .ok_or(KnownErrors::InvalidJournal)?;

        if journal_state.deleted {
            return Err(KnownErrors::InvalidJournal);
        }

        let invitee_id = self
            .user_store
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

        self.journal_store
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

    pub(crate) async fn journal_update_tenant_permissions(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        permissions: Permissions,
        actor: UserId,
    ) -> Result<(), KnownErrors> {
        let journal_state = self
            .journal_store
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

        self.journal_store
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

    pub(crate) async fn journal_remove_tenant(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        actor: UserId,
    ) -> Result<(), KnownErrors> {
        let journal_state = self
            .journal_store
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

        self.journal_store
            .record(
                journal_id,
                Authority::Direct(Actor::Anonymous),
                JournalEvent::RemovedTenant { id: target_user },
                None,
            )
            .await
    }

    pub(crate) async fn account_create(
        &self,
        account_id: AccountId,
        journal_id: JournalId,
        creator_id: UserId,
        account_name: String,
        parent_account_id: Option<AccountId>,
    ) -> Result<(), KnownErrors> {
        let journal_state = self
            .journal_store
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

        if self.account_store.get_account(&account_id).await?.is_some() {
            return Err(KnownErrors::AccountExists);
        }

        self.account_store
            .record(
                account_id,
                Authority::Direct(Actor::Anonymous),
                AccountEvent::Created {
                    journal_id,
                    name: account_name,
                    creator: creator_id,
                    created_at: Utc::now(),
                    parent_account_id,
                },
                None,
            )
            .await?;

        Ok(())
    }

    pub(crate) async fn account_get_all_in_journal(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Vec<(AccountId, AccountState)>, KnownErrors> {
        if !self
            .journal_store
            .get_permissions(journal_id, actor)
            .await?
            .is_some_and(|p| p.contains(Permissions::READ))
        {
            return Err(PermissionError {
                required_permissions: Permissions::READ,
            });
        }

        let ids = self.account_store.get_journal_accounts(journal_id).await?;

        let mut accounts = Vec::new();

        for id in ids {
            accounts.push((
                id,
                self.account_store
                    .get_account(&id)
                    .await?
                    .ok_or(KnownErrors::AccountDoesntExist { id })?,
            ));
        }

        Ok(accounts)
    }

    pub(crate) async fn account_get_full_path(
        &self,
        account_id: AccountId,
    ) -> Result<Option<Vec<String>>, KnownErrors> {
        let mut parts = Vec::new();
        let mut current_id = account_id;
        loop {
            match self.account_store.get_account(&current_id).await? {
                None => return Ok(None),
                Some(acc) => {
                    parts.push(acc.name);
                    match acc.parent_account_id {
                        Some(parent_id) => current_id = parent_id,
                        None => break,
                    }
                }
            }
        }
        parts.reverse();
        Ok(Some(parts))
    }

    pub(crate) async fn transaction_create(
        &self,
        transaction_id: TransactionId,
        journal_id: JournalId,
        creator_id: UserId,
        updates: Vec<BalanceUpdate>,
    ) -> Result<(), KnownErrors> {
        let journal_accounts = self.account_store.get_journal_accounts(journal_id).await?;

        for update in &updates {
            if !journal_accounts.contains(&update.account_id) {
                return Err(KnownErrors::AccountDoesntExist {
                    id: update.account_id,
                });
            }
        }

        if let Some(journal) = self.journal_store.get_journal(journal_id).await?
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
        self.account_store.update_balances(&event, None).await?;

        self.transaction_store
            .record(
                transaction_id,
                Authority::Direct(Actor::Anonymous),
                event,
                None,
            )
            .await?;

        Ok(())
    }

    pub(crate) async fn transaction_get_all_in_journal(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Vec<(TransactionId, TransactionState)>, KnownErrors> {
        if !self
            .journal_store
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
            .transaction_store
            .get_journal_transactions(journal_id)
            .await?;

        let mut transactions = Vec::new();

        for id in transaction_ids {
            transactions.push((
                id,
                self.transaction_store
                    .get_transaction(&id)
                    .await?
                    .ok_or(KnownErrors::InvalidTransaction { id })?,
            ));
        }

        Ok(transactions)
    }

    pub(crate) async fn user_get_email(
        &self,
        userid: UserId,
    ) -> Result<Option<String>, KnownErrors> {
        self.user_store
            .get_user_email(userid)
            .await
            .map_err(|e| KnownErrors::InternalError {
                context: e.to_string(),
            })
    }
}

pub type MemoryService =
    Service<MemoryUserStore, JournalMemoryStore, TransactionMemoryStore, AccountMemoryStore>;

impl Default for MemoryService {
    fn default() -> Self {
        Self::new(
            MemoryUserStore::new(),
            JournalMemoryStore::new(),
            TransactionMemoryStore::new(),
            AccountMemoryStore::new(),
        )
    }
}
