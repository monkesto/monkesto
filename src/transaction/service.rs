use crate::account::AccountService;
use crate::account::AccountStore;
use crate::auth::user::UserStore;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::authority::UserId;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::journal::JournalService;
use crate::journal::JournalStore;
use crate::journal::Permissions;
use crate::known_errors::KnownErrors;
use crate::known_errors::KnownErrors::PermissionError;
use crate::transaction::BalanceUpdate;
use crate::transaction::TransactionEvent;
use crate::transaction::TransactionState;
use crate::transaction::TransactionStore;
use chrono::Utc;

#[derive(Clone)]
pub struct TransactionService<T, A, J, U>
where
    T: TransactionStore,
    A: AccountStore,
    J: JournalStore,
    U: UserStore,
{
    transaction_store: T,
    account_service: AccountService<A, J, U>,
    journal_service: JournalService<J, U>,
}

impl<T, A, J, U> TransactionService<T, A, J, U>
where
    T: TransactionStore,
    A: AccountStore,
    J: JournalStore,
    U: UserStore,
{
    pub fn new(
        transaction_store: T,
        account_service: AccountService<A, J, U>,
        journal_service: JournalService<J, U>,
    ) -> Self {
        Self {
            transaction_store,
            account_service,
            journal_service,
        }
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
}
