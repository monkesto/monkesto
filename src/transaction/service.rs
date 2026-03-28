use crate::account::AccountService;
use crate::account::AccountStore;
use crate::auth::user::UserStore;
use crate::authority::Authority;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::journal::JournalService;
use crate::journal::JournalStore;
use crate::journal::Permissions;
use crate::transaction::BalanceUpdate;
use crate::transaction::TransactionPayload;
use crate::transaction::TransactionState;
use crate::transaction::TransactionStore;
use crate::transaction::TransactionStoreError::InvalidAccount;
use crate::transaction::TransactionStoreError::InvalidJournal;
use crate::transaction::TransactionStoreError::InvalidTransaction;
use crate::transaction::TransactionStoreError::PermissionError;
use crate::transaction::TransactionStoreResult;

#[derive(Clone)]
pub struct TransactionService<T, A, J, U>
where
    T: TransactionStore<EventId = u64>,
    A: AccountStore<EventId = u64>,
    J: JournalStore<EventId = u64>,
    U: UserStore<EventId = u64>,
{
    transaction_store: T,
    account_service: AccountService<A, J, U>,
    journal_service: JournalService<J, U>,
}

impl<T, A, J, U> TransactionService<T, A, J, U>
where
    T: TransactionStore<EventId = u64>,
    A: AccountStore<EventId = u64>,
    J: JournalStore<EventId = u64>,
    U: UserStore<EventId = u64>,
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

    pub async fn create_transaction(
        &self,
        transaction_id: TransactionId,
        journal_id: JournalId,
        authority: &Authority,
        updates: Vec<BalanceUpdate>,
    ) -> TransactionStoreResult<()> {
        // Check permission on the top-level journal (inherited by subjournals)
        let journal = self
            .journal_service
            .get_journal(journal_id, authority)
            .await?
            .ok_or(InvalidJournal(journal_id))?;

        if journal.deleted {
            return Err(InvalidJournal(journal_id));
        }

        let perms = self
            .journal_service
            .effective_permissions(journal_id, authority)
            .await?;

        if !perms.contains(Permissions::APPENDTRANSACTION) {
            return Err(PermissionError(Permissions::APPENDTRANSACTION));
        }

        // For each update, the account must belong to the entry's journal or any of its
        // ancestors (up to and including the root). Build the valid account set per entry.
        for update in &updates {
            let ancestor_ids = self
                .journal_service
                .get_ancestor_ids(update.journal_id)
                .await?;

            let mut valid = false;
            for ancestor_id in ancestor_ids {
                let accounts = self
                    .account_service
                    .store()
                    .get_journal_accounts(ancestor_id)
                    .await?;
                if accounts.contains(&update.account_id) {
                    valid = true;
                    break;
                }
            }

            if !valid {
                return Err(InvalidAccount(update.account_id));
            }
        }

        let payload = TransactionPayload::CreatedTransaction {
            journal_id,
            updates,
        };

        // update the balances first: this will check if the accounts actually exist
        self.account_service
            .store()
            .update_balances(transaction_id, &payload, None)
            .await?;

        self.transaction_store
            .record(transaction_id, authority.clone(), payload)
            .await?;

        Ok(())
    }

    pub async fn get_all_transactions_in_journal(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> TransactionStoreResult<Vec<(TransactionId, TransactionState)>> {
        let journal_state = self
            .journal_service
            .get_journal(journal_id, authority)
            .await?;

        if !journal_state.as_ref().is_some_and(|s| {
            s.get_actor_permissions(authority)
                .contains(Permissions::READ)
        }) {
            return Err(PermissionError(Permissions::READ));
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
                    .ok_or(InvalidTransaction(id))?,
            ));
        }

        Ok(transactions)
    }
}
