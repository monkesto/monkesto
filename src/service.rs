use crate::account::AccountMemoryStore;
use crate::account::AccountService;
use crate::account::AccountState;
use crate::account::AccountStore;
use crate::auth::MemoryUserStore;
use crate::auth::UserService;
use crate::auth::user::Email;
use crate::auth::user::UserStore;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::authority::UserId;
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::journal::JournalMemoryStore;
use crate::journal::JournalService;
use crate::journal::JournalState;
use crate::journal::JournalStore;
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
    user_service: UserService<U>,
    journal_service: JournalService<J, U>,
    account_service: AccountService<A, J, U>,
    transaction_store: T,
}

impl<U, J, T, A> Service<U, J, T, A>
where
    U: UserStore,
    J: JournalStore,
    T: TransactionStore,
    A: AccountStore,
{
    pub fn new(user_store: U, journal_store: J, transaction_store: T, account_store: A) -> Self {
        let user_service = UserService::new(user_store.clone());
        let journal_service = JournalService::new(journal_store, user_store);
        let account_service = AccountService::new(account_store, journal_service.clone());
        Self {
            user_service,
            journal_service,
            account_service,
            transaction_store,
        }
    }

    pub fn user_store(&self) -> &U {
        self.user_service.store()
    }

    pub async fn journal_create(
        &self,
        journal_id: JournalId,
        name: String,
        actor: UserId,
    ) -> Result<(), KnownErrors> {
        self.journal_service
            .journal_create(journal_id, name, actor)
            .await
    }

    pub async fn journal_list(
        &self,
        actor: UserId,
    ) -> Result<Vec<(JournalId, JournalState)>, KnownErrors> {
        self.journal_service.journal_list(actor).await
    }

    pub async fn journal_get(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Option<JournalState>, KnownErrors> {
        self.journal_service.journal_get(journal_id, actor).await
    }

    pub async fn journal_get_users(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Vec<UserId>, KnownErrors> {
        self.journal_service
            .journal_get_users(journal_id, actor)
            .await
    }

    pub async fn journal_get_name_from_res<E>(
        &self,
        journal_id_res: Result<JournalId, E>,
    ) -> Result<Option<String>, KnownErrors>
    where
        KnownErrors: From<E>,
    {
        self.journal_service
            .journal_get_name_from_res(journal_id_res)
            .await
    }

    pub async fn journal_invite_tenant(
        &self,
        journal_id: JournalId,
        actor: UserId,
        invitee: Email,
        permissions: Permissions,
    ) -> Result<(), KnownErrors> {
        self.journal_service
            .journal_invite_tenant(journal_id, actor, invitee, permissions)
            .await
    }

    pub async fn journal_update_tenant_permissions(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        permissions: Permissions,
        actor: UserId,
    ) -> Result<(), KnownErrors> {
        self.journal_service
            .journal_update_tenant_permissions(journal_id, target_user, permissions, actor)
            .await
    }

    pub async fn journal_remove_tenant(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        actor: UserId,
    ) -> Result<(), KnownErrors> {
        self.journal_service
            .journal_remove_tenant(journal_id, target_user, actor)
            .await
    }

    pub async fn account_create(
        &self,
        account_id: AccountId,
        journal_id: JournalId,
        creator_id: UserId,
        account_name: String,
        parent_account_id: Option<AccountId>,
    ) -> Result<(), KnownErrors> {
        self.account_service
            .account_create(
                account_id,
                journal_id,
                creator_id,
                account_name,
                parent_account_id,
            )
            .await
    }

    pub async fn account_get_all_in_journal(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Vec<(AccountId, AccountState)>, KnownErrors> {
        self.account_service
            .account_get_all_in_journal(journal_id, actor)
            .await
    }

    pub async fn account_get_full_path(
        &self,
        account_id: AccountId,
    ) -> Result<Option<Vec<String>>, KnownErrors> {
        self.account_service.account_get_full_path(account_id).await
    }

    pub async fn transaction_create(
        &self,
        transaction_id: TransactionId,
        journal_id: JournalId,
        creator_id: UserId,
        updates: Vec<BalanceUpdate>,
    ) -> Result<(), KnownErrors> {
        let journal_accounts = self
            .account_service
            .store()
            .get_journal_accounts(journal_id)
            .await?;

        for update in &updates {
            if !journal_accounts.contains(&update.account_id) {
                return Err(KnownErrors::AccountDoesntExist {
                    id: update.account_id,
                });
            }
        }

        let journal = self
            .journal_service
            .journal_get(journal_id, creator_id)
            .await?
            .ok_or(KnownErrors::InvalidJournal)?;

        if journal.deleted {
            return Err(KnownErrors::InvalidJournal);
        }

        if !journal
            .get_user_permissions(creator_id)
            .contains(Permissions::APPENDTRANSACTION)
        {
            return Err(PermissionError {
                required_permissions: Permissions::APPENDTRANSACTION,
            });
        }

        let event = TransactionEvent::CreatedTransaction {
            journal_id,
            author: creator_id,
            updates,
            created_at: Utc::now(),
        };

        // update the balances first: this will check if the accounts actually exist
        self.account_service
            .store()
            .update_balances(&event, None)
            .await?;

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

    pub async fn transaction_get_all_in_journal(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Vec<(TransactionId, TransactionState)>, KnownErrors> {
        let journal_state = self.journal_service.journal_get(journal_id, actor).await?;

        if !journal_state
            .as_ref()
            .is_some_and(|s| s.get_user_permissions(actor).contains(Permissions::READ))
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

    pub async fn user_get_email(&self, userid: UserId) -> Result<Option<String>, KnownErrors> {
        self.user_service.user_get_email(userid).await
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
