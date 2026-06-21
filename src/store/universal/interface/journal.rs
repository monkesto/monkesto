use crate::auth::user::UserId;
use crate::authority::Authority;
use crate::email::Email;
use crate::journal::{JournalId, JournalState, Permissions};
use crate::name::Name;
use crate::store::universal::error::{StoreError, StoreResult};

pub trait JournalInterface: Send + Sync + Clone + 'static {
    async fn create_journal(
        &self,
        name: Name,
        owner: UserId,
        authority: &Authority,
    ) -> StoreResult<JournalId>;

    async fn create_subjournal(
        &self,
        parent_journal_id: JournalId,
        name: Name,
        authority: &Authority,
    ) -> StoreResult<JournalId>;

    async fn get_ancestor_ids(&self, journal_id: JournalId) -> StoreResult<Vec<JournalId>>;

    async fn get_effective_permissions(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> StoreResult<Permissions>;

    /// A helper function that will return `StoreError::Permission` if the passed `authority` does not possess the specified permissions
    async fn validate_permissions(
        &self,
        journal_id: JournalId,
        authority: &Authority,
        required_permissions: Permissions,
    ) -> StoreResult<()> {
        if self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(required_permissions)
        {
            Ok(())
        } else {
            Err(StoreError::Permission(required_permissions))
        }
    }

    async fn list_accessible_top_level_journals(
        &self,
        user: UserId,
    ) -> StoreResult<Vec<JournalState>>;

    async fn get_journal(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> StoreResult<JournalState>;

    /// Returns only the direct children of `journal_id` (depth 1).
    async fn get_direct_subjournals(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> StoreResult<Vec<JournalId>>;

    /// Returns all descendants of `journal_id` at any depth (breadth-first), as a flat list.
    /// The list preserves parent-before-child ordering so callers can recurse through it.
    async fn get_descendants(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> StoreResult<Vec<JournalState>>;

    async fn invite_member(
        &self,
        journal_id: JournalId,
        invitee: Email,
        permissions: Permissions,
        authority: &Authority,
    ) -> StoreResult<()>;

    async fn update_member_permissions(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        permissions: Permissions,
        authority: &Authority,
    ) -> StoreResult<()>;

    async fn remove_member(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        authority: &Authority,
    ) -> StoreResult<()>;

    async fn get_creator(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> StoreResult<Authority>;
}

#[cfg(test)]
mod tests {
    use crate::authority::{Actor, Authority};
    use crate::email::Email;
    use crate::journal::Permissions;
    use crate::store::universal::diesel_sqlite::DieselSqliteStore;
    use crate::store::universal::diesel_sqlite_interface::{
        DieselSqliteAuthInterface, DieselSqliteJournalInterface,
    };
    use crate::store::universal::error::StoreError;
    use crate::store::universal::interface::auth::AuthInterface;
    use crate::store::universal::interface::journal::JournalInterface;
    use crate::store::universal::interface::{TEST_AUTHORITY, TEST_EMAIL, TEST_JOURNAL_NAME};
    use uuid::Uuid;

    async fn interfaces() -> (DieselSqliteAuthInterface, DieselSqliteJournalInterface) {
        let store = DieselSqliteStore::new(":memory:", 1).await;

        let (_, auth_interface, journal_interface, _) = store.interfaces().await;

        (auth_interface, journal_interface)
    }

    #[tokio::test]
    async fn journal_creation() {
        let (auth_interface, journal_interface) = interfaces().await;

        let owner_id = auth_interface
            .create_user(TEST_EMAIL.clone(), Uuid::new_v4(), &TEST_AUTHORITY.clone())
            .await
            .unwrap();
        let owner_authority = &Authority::Direct(Actor::User(owner_id));

        let journal_id = journal_interface
            .create_journal(TEST_JOURNAL_NAME.clone(), owner_id, owner_authority)
            .await
            .unwrap();

        let journal_state = journal_interface
            .get_journal(journal_id, owner_authority)
            .await
            .unwrap();
        assert_eq!(journal_state.owner, owner_id);

        let creator = journal_interface
            .get_creator(journal_id, owner_authority)
            .await
            .unwrap();
        assert_eq!(&creator, owner_authority);
    }

    #[tokio::test]
    async fn subjournal_creation() {
        let (auth_interface, journal_interface) = interfaces().await;

        let owner_id = auth_interface
            .create_user(TEST_EMAIL.clone(), Uuid::new_v4(), &TEST_AUTHORITY.clone())
            .await
            .unwrap();
        let owner_authority = &Authority::Direct(Actor::User(owner_id));

        let subjournal_owner_email = Email::try_new("test2@example.com".to_string()).unwrap();
        let subjournal_owner_id = auth_interface
            .create_user(
                subjournal_owner_email.clone(),
                Uuid::new_v4(),
                &TEST_AUTHORITY.clone(),
            )
            .await
            .unwrap();
        let subjournal_owner_authority = &Authority::Direct(Actor::User(subjournal_owner_id));

        let journal_id = journal_interface
            .create_journal(TEST_JOURNAL_NAME.clone(), owner_id, owner_authority)
            .await
            .unwrap();

        journal_interface
            .invite_member(
                journal_id,
                subjournal_owner_email,
                Permissions::READ | Permissions::CREATE_SUBJOURNAL,
                owner_authority,
            )
            .await
            .unwrap();

        let subjournal_id = journal_interface
            .create_subjournal(
                journal_id,
                TEST_JOURNAL_NAME.clone(),
                subjournal_owner_authority,
            )
            .await
            .unwrap();

        let journal_state = journal_interface
            .get_journal(subjournal_id, owner_authority)
            .await
            .unwrap();
        // subjournals currently inherit their owner from the parent journal
        assert_eq!(journal_state.owner, owner_id);

        // creator should not be inherited
        let creator = journal_interface
            .get_creator(subjournal_id, owner_authority)
            .await
            .unwrap();
        assert_eq!(&creator, subjournal_owner_authority);
    }

    #[tokio::test]
    async fn member_permissions() {
        let (auth_interface, journal_interface) = interfaces().await;

        let owner_id = auth_interface
            .create_user(TEST_EMAIL.clone(), Uuid::new_v4(), &TEST_AUTHORITY.clone())
            .await
            .unwrap();
        let owner_authority = &Authority::Direct(Actor::User(owner_id));

        let member_email = Email::try_new("test2@example.com".to_string()).unwrap();
        let member_id = auth_interface
            .create_user(
                member_email.clone(),
                Uuid::new_v4(),
                &TEST_AUTHORITY.clone(),
            )
            .await
            .unwrap();
        let member_authority = &Authority::Direct(Actor::User(member_id));

        let journal_id = journal_interface
            .create_journal(TEST_JOURNAL_NAME.clone(), owner_id, owner_authority)
            .await
            .unwrap();

        assert_eq!(
            journal_interface
                .get_journal(journal_id, member_authority)
                .await,
            Err(StoreError::EntityNotFound)
        );

        journal_interface
            .invite_member(journal_id, member_email, Permissions::READ, owner_authority)
            .await
            .unwrap();
        assert!(
            journal_interface
                .get_journal(journal_id, member_authority)
                .await
                .is_ok()
        );

        assert_eq!(
            journal_interface
                .validate_permissions(
                    journal_id,
                    member_authority,
                    Permissions::ADD_ACCOUNT | Permissions::CREATE_SUBJOURNAL
                )
                .await,
            Err(StoreError::Permission(
                Permissions::ADD_ACCOUNT | Permissions::CREATE_SUBJOURNAL
            ))
        );

        journal_interface
            .update_member_permissions(
                journal_id,
                member_id,
                Permissions::READ | Permissions::ADD_ACCOUNT | Permissions::CREATE_SUBJOURNAL,
                owner_authority,
            )
            .await
            .unwrap();

        assert!(
            journal_interface
                .validate_permissions(
                    journal_id,
                    member_authority,
                    Permissions::ADD_ACCOUNT | Permissions::CREATE_SUBJOURNAL
                )
                .await
                .is_ok()
        );

        journal_interface
            .remove_member(journal_id, member_id, owner_authority)
            .await
            .unwrap();
        assert_eq!(
            journal_interface
                .get_journal(journal_id, member_authority)
                .await,
            Err(StoreError::EntityNotFound)
        );
    }
}
