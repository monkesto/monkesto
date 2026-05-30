use crate::account::{AccountId, AccountState};
use crate::authority::Authority;
use crate::journal::JournalId;
use crate::name::Name;
use crate::store::universal::error::StoreResult;

pub trait AccountInterface: Sync + Send + Clone + 'static {
    async fn create_account(
        &self,
        journal_id: JournalId,
        name: Name,
        authority: &Authority,
    ) -> StoreResult<AccountId>;
    async fn get_account(&self, account_id: AccountId) -> StoreResult<AccountState>;
    async fn get_accounts_in_journal(&self, journal_id: JournalId) -> StoreResult<Vec<AccountId>>;
}
