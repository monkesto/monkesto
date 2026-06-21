use crate::account::AccountState;
use crate::account::AccountStore;
use crate::account::AccountStoreError::AccountExists;
use crate::account::AccountStoreError::InvalidAccount;
use crate::account::AccountStoreError::PermissionError;
use crate::account::AccountStoreResult;
use crate::account::{AccountId, AccountPayload};
use crate::auth::user::UserStore;
use crate::authority::Authority;
use crate::journal::JournalStore;
use crate::journal::Permissions;
use crate::journal::{JournalId, JournalService};
use crate::name::Name;

#[derive(Clone)]
pub struct AccountService<A, J, U>
where
    A: AccountStore<EventId = u64>,
    J: JournalStore<EventId = u64>,
    U: UserStore<EventId = u64>,
{
    account_store: A,
    journal_service: JournalService<J, U>,
}

impl<A, J, U> AccountService<A, J, U>
where
    A: AccountStore<EventId = u64>,
    J: JournalStore<EventId = u64>,
    U: UserStore<EventId = u64>,
{
    pub fn new(account_store: A, journal_service: JournalService<J, U>) -> Self {
        Self {
            account_store,
            journal_service,
        }
    }

    pub async fn create_account(
        &self,
        account_id: AccountId,
        journal_id: JournalId,
        authority: &Authority,
        account_name: Name,
        parent_account_id: Option<AccountId>,
    ) -> AccountStoreResult<()> {
        if !self
            .journal_service
            .effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::ADD_ACCOUNT)
        {
            return Err(PermissionError(Permissions::ADD_ACCOUNT));
        }

        if self.account_store.get_account(&account_id).await?.is_some() {
            return Err(AccountExists(account_id));
        }

        self.account_store
            .record(
                account_id,
                authority.clone(),
                AccountPayload::Created {
                    journal_id,
                    name: account_name,
                    parent_account_id,
                },
            )
            .await?;

        Ok(())
    }

    pub async fn get_all_accounts_in_journal(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> AccountStoreResult<Vec<(AccountId, AccountState)>> {
        if !self
            .journal_service
            .effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(PermissionError(Permissions::READ));
        }

        let ids = self.account_store.get_journal_accounts(journal_id).await?;

        let mut accounts = Vec::new();

        for id in ids {
            accounts.push((
                id,
                self.account_store
                    .get_account(&id)
                    .await?
                    .ok_or(InvalidAccount(id))?,
            ));
        }

        Ok(accounts)
    }

    pub async fn get_full_account_path(
        &self,
        account_id: AccountId,
    ) -> AccountStoreResult<Option<Vec<Name>>> {
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

    pub fn store(&self) -> &A {
        &self.account_store
    }
}
