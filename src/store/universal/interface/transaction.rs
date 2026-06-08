use crate::authority::Authority;
use crate::journal::JournalId;
use crate::store::universal::error::StoreResult;
use crate::transaction::{BalanceUpdate, TransactionId, TransactionState};

pub trait TransactionInterface: Send + Sync + Clone + 'static {
    async fn create_transaction(
        &self,
        journal_id: JournalId,
        updates: Vec<BalanceUpdate>,
        authority: &Authority,
    ) -> StoreResult<TransactionId>;

    async fn get_all_in_journal(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> StoreResult<Vec<TransactionState>>;

    async fn get_creator(
        &self,
        transaction_id: TransactionId,
        authority: &Authority,
    ) -> StoreResult<Authority>;
}
