use crate::account::AccountEvent;
use crate::account::AccountState;
use crate::account::AccountStore;
use crate::auth::user::UserStore;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::authority::UserId;
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::journal::JournalService;
use crate::journal::JournalStore;
use crate::journal::Permissions;
use crate::known_errors::KnownErrors;
use crate::known_errors::KnownErrors::PermissionError;
use chrono::Utc;

#[derive(Clone)]
pub struct AccountService<A, J, U>
where
    A: AccountStore,
    J: JournalStore,
    U: UserStore,
{
    account_store: A,
    journal_service: JournalService<J, U>,
}

impl<A, J, U> AccountService<A, J, U>
where
    A: AccountStore,
    J: JournalStore,
    U: UserStore,
{
    pub fn new(account_store: A, journal_service: JournalService<J, U>) -> Self {
        Self {
            account_store,
            journal_service,
        }
    }

    pub async fn account_create(
        &self,
        account_id: AccountId,
        journal_id: JournalId,
        creator_id: UserId,
        account_name: String,
        parent_account_id: Option<AccountId>,
    ) -> Result<(), KnownErrors> {
        let journal_state = self
            .journal_service
            .journal_get(journal_id, creator_id)
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
            )
            .await?;

        Ok(())
    }

    pub async fn account_get_all_in_journal(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Vec<(AccountId, AccountState)>, KnownErrors> {
        let journal_state = self.journal_service.journal_get(journal_id, actor).await?;

        if !journal_state
            .as_ref()
            .is_some_and(|s| s.get_user_permissions(actor).contains(Permissions::READ))
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

    pub async fn account_get_full_path(
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

    pub fn store(&self) -> &A {
        &self.account_store
    }
}
