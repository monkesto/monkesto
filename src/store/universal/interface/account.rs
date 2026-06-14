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

    async fn create_subaccount(
        &self,
        parent_account_id: AccountId,
        journal_id: JournalId,
        name: Name,
        authority: &Authority,
    ) -> StoreResult<AccountId>;

    async fn get_account(&self, account_id: AccountId) -> StoreResult<AccountState>;
    async fn get_accounts_in_journal(&self, journal_id: JournalId) -> StoreResult<Vec<AccountId>>;
}

#[cfg(test)]
mod tests {
    use crate::authority::{Actor, Authority};
    use crate::email::Email;
    use crate::journal::{JournalId, Permissions};
    use crate::name::Name;
    use crate::store::universal::diesel_sqlite::DieselSqliteStore;
    use crate::store::universal::diesel_sqlite_interface::{
        DieselSqliteAccountInterface, DieselSqliteAuthInterface, DieselSqliteJournalInterface,
    };
    use crate::store::universal::error::StoreError;
    use crate::store::universal::interface::account::AccountInterface;
    use crate::store::universal::interface::auth::AuthInterface;
    use crate::store::universal::interface::journal::JournalInterface;
    use std::sync::LazyLock;

    async fn interfaces() -> (
        DieselSqliteAuthInterface,
        DieselSqliteAccountInterface,
        DieselSqliteJournalInterface,
    ) {
        let store = DieselSqliteStore::new(":memory:", 1).await;

        let (account_interface, auth_interface, journal_interface, _) = store.interfaces().await;

        (auth_interface, account_interface, journal_interface)
    }

    static TEST_ACCT_NAME: LazyLock<Name> =
        LazyLock::new(|| Name::try_new("test account".to_string()).unwrap());

    static TEST_JOURNAL_NAME: LazyLock<Name> =
        LazyLock::new(|| Name::try_new("test account".to_string()).unwrap());

    const TEST_AUTHORITY: Authority = Authority::Direct(Actor::System);

    #[tokio::test]
    async fn account_creation() {
        let (auth_interface, acct_interface, journal_interface) = interfaces().await;

        let user_id = auth_interface
            .create_user(
                Email::try_new("test@example.com".to_string()).unwrap(),
                webauthn_rs::prelude::Uuid::default(),
                &TEST_AUTHORITY,
            )
            .await
            .unwrap();

        let journal_id = journal_interface
            .create_journal(TEST_JOURNAL_NAME.clone(), user_id, &TEST_AUTHORITY)
            .await
            .unwrap();

        let account_id = acct_interface
            .create_account(journal_id, TEST_ACCT_NAME.clone(), &TEST_AUTHORITY)
            .await
            .unwrap();

        let state = acct_interface.get_account(account_id).await.unwrap();

        assert_eq!(state.name, TEST_ACCT_NAME.clone());
        assert_eq!(state.id, account_id);
        assert_eq!(state.parent_account_id, None);
        assert_eq!(state.balance, 0);
    }

    #[tokio::test]
    async fn account_creation_invalid_journal() {
        let (_, acct_interface, _) = interfaces().await;

        assert_eq!(
            acct_interface
                .create_account(JournalId::new(), TEST_ACCT_NAME.clone(), &TEST_AUTHORITY)
                .await,
            Err(StoreError::EntityDoesntExist)
        );
    }

    #[tokio::test]
    async fn account_creation_sufficient_permissions() {
        let (auth_interface, acct_interface, journal_interface) = interfaces().await;

        let owner_id = auth_interface
            .create_user(
                Email::try_new("test@example.com".to_string()).unwrap(),
                webauthn_rs::prelude::Uuid::default(),
                &TEST_AUTHORITY,
            )
            .await
            .unwrap();

        let sufficient_perms_email = Email::try_new("test2@example.com".to_string()).unwrap();

        let sufficient_perms_id = auth_interface
            .create_user(
                sufficient_perms_email.clone(),
                webauthn_rs::prelude::Uuid::default(),
                &TEST_AUTHORITY,
            )
            .await
            .unwrap();

        let journal_id = journal_interface
            .create_journal(TEST_JOURNAL_NAME.clone(), owner_id, &TEST_AUTHORITY)
            .await
            .unwrap();

        journal_interface
            .invite_member(
                journal_id,
                sufficient_perms_email,
                Permissions::ADDACCOUNT,
                &TEST_AUTHORITY,
            )
            .await
            .unwrap();

        acct_interface
            .create_account(
                journal_id,
                TEST_ACCT_NAME.clone(),
                &Authority::Direct(Actor::User(sufficient_perms_id)),
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn account_creation_insufficient_permissions() {
        let (auth_interface, acct_interface, journal_interface) = interfaces().await;

        let owner_id = auth_interface
            .create_user(
                Email::try_new("test@example.com".to_string()).unwrap(),
                webauthn_rs::prelude::Uuid::default(),
                &TEST_AUTHORITY,
            )
            .await
            .unwrap();

        let owner_authority = &Authority::Direct(Actor::User(owner_id));

        let insufficient_perms_email = Email::try_new("test2@example.com".to_string()).unwrap();

        let insufficient_perms_id = auth_interface
            .create_user(
                insufficient_perms_email.clone(),
                webauthn_rs::prelude::Uuid::default(),
                &TEST_AUTHORITY,
            )
            .await
            .unwrap();

        let journal_id = journal_interface
            .create_journal(TEST_JOURNAL_NAME.clone(), owner_id, owner_authority)
            .await
            .unwrap();

        journal_interface
            .invite_member(
                journal_id,
                insufficient_perms_email,
                Permissions::READ,
                owner_authority,
            )
            .await
            .unwrap();

        assert_eq!(
            acct_interface
                .create_account(
                    journal_id,
                    TEST_ACCT_NAME.clone(),
                    &Authority::Direct(Actor::User(insufficient_perms_id))
                )
                .await,
            Err(StoreError::Permission(Permissions::ADDACCOUNT))
        );
    }

    #[tokio::test]
    async fn account_creation_no_permissions() {
        let (auth_interface, acct_interface, journal_interface) = interfaces().await;

        let owner_id = auth_interface
            .create_user(
                Email::try_new("test@example.com".to_string()).unwrap(),
                webauthn_rs::prelude::Uuid::default(),
                &TEST_AUTHORITY,
            )
            .await
            .unwrap();

        let owner_authority = &Authority::Direct(Actor::User(owner_id));

        let no_perms_id = auth_interface
            .create_user(
                Email::try_new("test2@example.com".to_string()).unwrap(),
                webauthn_rs::prelude::Uuid::default(),
                &TEST_AUTHORITY,
            )
            .await
            .unwrap();

        let journal_id = journal_interface
            .create_journal(TEST_JOURNAL_NAME.clone(), owner_id, owner_authority)
            .await
            .unwrap();

        // the store should pretend that the journal doesn't exist if they don't have read access
        assert_eq!(
            acct_interface
                .create_account(
                    journal_id,
                    TEST_ACCT_NAME.clone(),
                    &Authority::Direct(Actor::User(no_perms_id))
                )
                .await,
            Err(StoreError::EntityDoesntExist)
        );
    }
}
