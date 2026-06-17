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

#[cfg(test)]
mod tests {
    use crate::authority::{Actor, Authority, UserId};
    use crate::email::Email;
    use crate::journal::Permissions;
    use crate::name::Name;
    use crate::store::universal::diesel_sqlite::DieselSqliteStore;
    use crate::store::universal::diesel_sqlite_interface::{
        DieselSqliteAccountInterface, DieselSqliteAuthInterface, DieselSqliteJournalInterface,
        DieselSqliteTransactionInterface,
    };
    use crate::store::universal::error::StoreError;
    use crate::store::universal::interface::account::AccountInterface;
    use crate::store::universal::interface::auth::AuthInterface;
    use crate::store::universal::interface::journal::JournalInterface;
    use crate::store::universal::interface::transaction::TransactionInterface;
    use crate::store::universal::interface::{TEST_AUTHORITY, TEST_JOURNAL_NAME};
    use crate::transaction::{BalanceUpdate, EntryType};
    use std::sync::LazyLock;
    use uuid::Uuid;

    static PRIMARY_TEST_EMAIL: LazyLock<Email> =
        LazyLock::new(|| Email::try_new("test@example.com").unwrap());

    static SECONDARY_TEST_EMAIL: LazyLock<Email> =
        LazyLock::new(|| Email::try_new("test2@example.com").unwrap());

    static TEST_ACCOUNT_NAME_ONE: LazyLock<Name> =
        LazyLock::new(|| Name::try_new("test account one".to_string()).unwrap());

    static TEST_ACCOUNT_NAME_TWO: LazyLock<Name> =
        LazyLock::new(|| Name::try_new("test account two".to_string()).unwrap());

    async fn interfaces() -> (
        DieselSqliteAccountInterface,
        DieselSqliteAuthInterface,
        DieselSqliteJournalInterface,
        DieselSqliteTransactionInterface,
    ) {
        let store = DieselSqliteStore::new(":memory:", 1).await;
        store.interfaces().await
    }

    #[tokio::test]
    async fn transaction_creation() {
        let (acct_interface, auth_interface, journal_interface, transaction_interface) =
            interfaces().await;
        let user_id = auth_interface
            .create_user(PRIMARY_TEST_EMAIL.clone(), Uuid::new_v4(), &TEST_AUTHORITY)
            .await
            .unwrap();
        let user_authority = &Authority::Direct(Actor::User(user_id));
        let journal_id = journal_interface
            .create_journal(TEST_JOURNAL_NAME.clone(), user_id, user_authority)
            .await
            .unwrap();
        let account_one_id = acct_interface
            .create_account(journal_id, TEST_ACCOUNT_NAME_ONE.clone(), user_authority)
            .await
            .unwrap();
        let account_two_id = acct_interface
            .create_account(journal_id, TEST_ACCOUNT_NAME_TWO.clone(), user_authority)
            .await
            .unwrap();

        let updates = vec![
            BalanceUpdate {
                journal_id,
                account_id: account_one_id,
                amount: 100,
                entry_type: EntryType::Credit,
            },
            BalanceUpdate {
                journal_id,
                account_id: account_two_id,
                amount: 100,
                entry_type: EntryType::Debit,
            },
        ];

        let transaction_id = transaction_interface
            .create_transaction(journal_id, updates.clone(), user_authority)
            .await
            .unwrap();

        let journal_transaction = transaction_interface
            .get_all_in_journal(journal_id, user_authority)
            .await
            .unwrap()
            .first()
            .unwrap()
            .clone();

        assert_eq!(
            transaction_interface
                .get_creator(journal_transaction.id, &TEST_AUTHORITY.clone())
                .await
                .unwrap(),
            user_authority.clone()
        );

        assert_eq!(journal_transaction.id, transaction_id);
        assert_eq!(journal_transaction.updates.0, updates);

        let account_one_balance = acct_interface
            .get_account(account_one_id)
            .await
            .unwrap()
            .balance;
        assert_eq!(account_one_balance, 100);

        let account_two_balance = acct_interface
            .get_account(account_two_id)
            .await
            .unwrap()
            .balance;
        assert_eq!(account_two_balance, -100);
    }

    #[tokio::test]
    async fn transaction_creation_sufficient_permissions() {
        let (acct_interface, auth_interface, journal_interface, transaction_interface) =
            interfaces().await;
        let owner_id = auth_interface
            .create_user(PRIMARY_TEST_EMAIL.clone(), Uuid::new_v4(), &TEST_AUTHORITY)
            .await
            .unwrap();
        let sufficient_perms_id = auth_interface
            .create_user(
                SECONDARY_TEST_EMAIL.clone(),
                Uuid::new_v4(),
                &TEST_AUTHORITY,
            )
            .await
            .unwrap();

        let owner_authority = &Authority::Direct(Actor::User(owner_id));
        let sufficient_perms_authority = &Authority::Direct(Actor::User(sufficient_perms_id));

        let journal_id = journal_interface
            .create_journal(TEST_JOURNAL_NAME.clone(), owner_id, owner_authority)
            .await
            .unwrap();
        journal_interface
            .invite_member(
                journal_id,
                SECONDARY_TEST_EMAIL.clone(),
                Permissions::READ | Permissions::APPENDTRANSACTION,
                owner_authority,
            )
            .await
            .unwrap();

        let account_one_id = acct_interface
            .create_account(journal_id, TEST_ACCOUNT_NAME_ONE.clone(), owner_authority)
            .await
            .unwrap();
        let account_two_id = acct_interface
            .create_account(journal_id, TEST_ACCOUNT_NAME_TWO.clone(), owner_authority)
            .await
            .unwrap();

        let updates = vec![
            BalanceUpdate {
                journal_id,
                account_id: account_one_id,
                amount: 100,
                entry_type: EntryType::Credit,
            },
            BalanceUpdate {
                journal_id,
                account_id: account_two_id,
                amount: 100,
                entry_type: EntryType::Debit,
            },
        ];

        let transaction_id = transaction_interface
            .create_transaction(journal_id, updates.clone(), sufficient_perms_authority)
            .await
            .unwrap();

        let journal_transaction = transaction_interface
            .get_all_in_journal(journal_id, sufficient_perms_authority)
            .await
            .unwrap()
            .first()
            .unwrap()
            .clone();

        assert_eq!(
            transaction_interface
                .get_creator(journal_transaction.id, &TEST_AUTHORITY.clone())
                .await
                .unwrap(),
            sufficient_perms_authority.clone()
        );

        assert_eq!(journal_transaction.id, transaction_id);
        assert_eq!(journal_transaction.updates.0, updates);

        let account_one_balance = acct_interface
            .get_account(account_one_id)
            .await
            .unwrap()
            .balance;
        assert_eq!(account_one_balance, 100);

        let account_two_balance = acct_interface
            .get_account(account_two_id)
            .await
            .unwrap()
            .balance;
        assert_eq!(account_two_balance, -100);
    }

    #[tokio::test]
    async fn transaction_creation_insufficient_permissions() {
        let (acct_interface, auth_interface, journal_interface, transaction_interface) =
            interfaces().await;

        let owner_id = auth_interface
            .create_user(PRIMARY_TEST_EMAIL.clone(), Uuid::new_v4(), &TEST_AUTHORITY)
            .await
            .unwrap();
        let sufficient_perms_id = auth_interface
            .create_user(
                SECONDARY_TEST_EMAIL.clone(),
                Uuid::new_v4(),
                &TEST_AUTHORITY,
            )
            .await
            .unwrap();

        let owner_authority = &Authority::Direct(Actor::User(owner_id));
        let insufficient_perms_authority = &Authority::Direct(Actor::User(sufficient_perms_id));

        let journal_id = journal_interface
            .create_journal(TEST_JOURNAL_NAME.clone(), owner_id, owner_authority)
            .await
            .unwrap();
        journal_interface
            .invite_member(
                journal_id,
                SECONDARY_TEST_EMAIL.clone(),
                Permissions::READ,
                owner_authority,
            )
            .await
            .unwrap();

        let account_one_id = acct_interface
            .create_account(journal_id, TEST_ACCOUNT_NAME_ONE.clone(), owner_authority)
            .await
            .unwrap();
        let account_two_id = acct_interface
            .create_account(journal_id, TEST_ACCOUNT_NAME_TWO.clone(), owner_authority)
            .await
            .unwrap();

        let updates = vec![
            BalanceUpdate {
                journal_id,
                account_id: account_one_id,
                amount: 100,
                entry_type: EntryType::Credit,
            },
            BalanceUpdate {
                journal_id,
                account_id: account_two_id,
                amount: 100,
                entry_type: EntryType::Debit,
            },
        ];

        assert_eq!(
            transaction_interface
                .create_transaction(journal_id, updates.clone(), insufficient_perms_authority)
                .await,
            Err(StoreError::Permission(Permissions::APPENDTRANSACTION))
        );

        assert!(
            transaction_interface
                .get_all_in_journal(journal_id, insufficient_perms_authority)
                .await
                .unwrap()
                .is_empty()
        );

        let account_one_balance = acct_interface
            .get_account(account_one_id)
            .await
            .unwrap()
            .balance;
        assert_eq!(account_one_balance, 0);

        let account_two_balance = acct_interface
            .get_account(account_two_id)
            .await
            .unwrap()
            .balance;
        assert_eq!(account_two_balance, 0);
    }

    #[tokio::test]
    async fn transaction_creation_no_permission() {
        let (acct_interface, auth_interface, journal_interface, transaction_interface) =
            interfaces().await;
        let owner_id = auth_interface
            .create_user(PRIMARY_TEST_EMAIL.clone(), Uuid::new_v4(), &TEST_AUTHORITY)
            .await
            .unwrap();

        let owner_authority = &Authority::Direct(Actor::User(owner_id));
        let journal_id = journal_interface
            .create_journal(TEST_JOURNAL_NAME.clone(), owner_id, owner_authority)
            .await
            .unwrap();
        let account_one_id = acct_interface
            .create_account(journal_id, TEST_ACCOUNT_NAME_ONE.clone(), owner_authority)
            .await
            .unwrap();
        let account_two_id = acct_interface
            .create_account(journal_id, TEST_ACCOUNT_NAME_TWO.clone(), owner_authority)
            .await
            .unwrap();

        let updates = vec![
            BalanceUpdate {
                journal_id,
                account_id: account_one_id,
                amount: 100,
                entry_type: EntryType::Credit,
            },
            BalanceUpdate {
                journal_id,
                account_id: account_two_id,
                amount: 100,
                entry_type: EntryType::Debit,
            },
        ];

        assert_eq!(
            transaction_interface
                .create_transaction(
                    journal_id,
                    updates.clone(),
                    &Authority::Direct(Actor::User(UserId::new()))
                )
                .await,
            Err(StoreError::EntityDoesntExist)
        );

        assert!(
            transaction_interface
                .get_all_in_journal(journal_id, owner_authority)
                .await
                .unwrap()
                .is_empty()
        );

        let account_one_balance = acct_interface
            .get_account(account_one_id)
            .await
            .unwrap()
            .balance;
        assert_eq!(account_one_balance, 0);

        let account_two_balance = acct_interface
            .get_account(account_two_id)
            .await
            .unwrap()
            .balance;
        assert_eq!(account_two_balance, 0);
    }
}
