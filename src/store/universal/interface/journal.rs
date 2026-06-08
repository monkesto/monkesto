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
