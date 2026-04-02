use crate::auth::user::Email;
use crate::auth::user::UserStore;
use crate::authority::Authority;
use crate::authority::UserId;
use crate::ident::JournalId;
use crate::journal::JounalPayload;
use crate::journal::JournalState;
use crate::journal::JournalStore;
use crate::journal::JournalStoreError;
use crate::journal::JournalStoreError::InvalidJournal;
use crate::journal::JournalStoreError::InvalidUser;
use crate::journal::JournalStoreError::PermissionError;
use crate::journal::JournalStoreError::UserAlreadyHasAccess;
use crate::journal::JournalStoreError::UserDoesntHaveAccess;
use crate::journal::JournalStoreError::UserLookupFailed;
use crate::journal::JournalStoreResult;
use crate::journal::Permissions;
use crate::name::Name;
use chrono::DateTime;
use chrono::Utc;

#[derive(Clone)]
pub struct JournalService<J, U>
where
    J: JournalStore<EventId = u64>,
    U: UserStore<EventId = u64>,
{
    journal_store: J,
    user_store: U,
}

impl<J, U> JournalService<J, U>
where
    J: JournalStore<EventId = u64>,
    U: UserStore<EventId = u64>,
{
    pub fn new(journal_store: J, user_store: U) -> Self {
        Self {
            journal_store,
            user_store,
        }
    }

    pub async fn create_journal(
        &self,
        journal_id: JournalId,
        name: Name,
        owner: UserId,
        authority: &Authority,
    ) -> JournalStoreResult<u64> {
        self.journal_store
            .record(
                journal_id,
                authority.clone(),
                JounalPayload::Created {
                    name,
                    parent_journal_id: None,
                    owner,
                },
            )
            .await
    }

    pub async fn create_subjournal(
        &self,
        parent_journal_id: JournalId,
        name: Name,
        authority: &Authority,
    ) -> JournalStoreResult<JournalId> {
        let parent_state = self
            .journal_store
            .get_journal(parent_journal_id)
            .await?
            .ok_or(InvalidJournal(parent_journal_id))?;

        if parent_state.deleted {
            return Err(InvalidJournal(parent_journal_id));
        }

        if !parent_state
            .get_actor_permissions(authority)
            .contains(Permissions::ADDACCOUNT)
        {
            return Err(PermissionError(Permissions::ADDACCOUNT));
        }

        let subjournal_id = JournalId::new();
        self.journal_store
            .record(
                subjournal_id,
                authority.clone(),
                JounalPayload::Created {
                    name,
                    // TODO: figure out how to handle journal/subjournal ownership
                    owner: parent_state.owner,
                    parent_journal_id: Some(parent_journal_id),
                },
            )
            .await?;
        Ok(subjournal_id)
    }

    /// Returns `journal_id` and all its ancestors up to (and including) the root, in
    /// child-first order (i.e. the given journal comes first, root comes last).
    pub async fn get_ancestor_ids(
        &self,
        journal_id: JournalId,
    ) -> JournalStoreResult<Vec<JournalId>> {
        let mut chain = Vec::new();
        let mut current_id = journal_id;
        loop {
            chain.push(current_id);
            let state = self
                .journal_store
                .get_journal(current_id)
                .await?
                .ok_or(InvalidJournal(journal_id))?;
            match state.parent_journal_id {
                Some(parent_id) => current_id = parent_id,
                None => break,
            }
        }
        Ok(chain)
    }

    /// Resolves the effective permissions for `actor` on `journal_id`, walking up
    /// the parent chain so that subjournals inherit their parent's permissions.
    pub async fn effective_permissions(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> JournalStoreResult<Permissions> {
        let mut current_id = journal_id;
        loop {
            let state = self
                .journal_store
                .get_journal(current_id)
                .await?
                .ok_or(InvalidJournal(journal_id))?;

            let perms = state.get_actor_permissions(authority);
            if !perms.is_empty() {
                return Ok(perms);
            }

            match state.parent_journal_id {
                Some(parent_id) => current_id = parent_id,
                None => return Ok(Permissions::empty()),
            }
        }
    }

    pub async fn list_journals(
        &self,
        actor: UserId,
    ) -> JournalStoreResult<Vec<(JournalId, JournalState)>> {
        let ids = self.journal_store.get_user_journals(actor).await?;

        let mut journals = Vec::new();

        for id in ids {
            let state = self
                .journal_store
                .get_journal(id)
                .await?
                .ok_or(InvalidJournal(id))?;

            // Only show top-level journals; subjournals are accessed through their parent
            if state.parent_journal_id.is_none() {
                journals.push((id, state));
            }
        }

        Ok(journals)
    }

    pub async fn get_journal(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> JournalStoreResult<Option<JournalState>> {
        let state = match self.journal_store.get_journal(journal_id).await? {
            Some(s) => s,
            None => return Ok(None),
        };

        let perms = self.effective_permissions(journal_id, authority).await?;
        if perms.contains(Permissions::READ) {
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    pub async fn get_journal_members(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalStoreResult<Vec<UserId>> {
        let journal_state = self
            .journal_store
            .get_journal(journal_id)
            .await?
            .ok_or(InvalidJournal(journal_id))?;

        if !journal_state
            .get_actor_permissions(&authority)
            .contains(Permissions::READ)
        {
            return Err(PermissionError(Permissions::READ));
        }

        let mut users = Vec::new();

        for (user_id, _) in journal_state.members.iter() {
            users.push(*user_id);
        }

        users.push(journal_state.owner);

        Ok(users)
    }

    /// Returns only the direct children of `journal_id` (depth 1).
    pub async fn get_direct_subjournals(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> JournalStoreResult<Vec<(JournalId, JournalState)>> {
        let perms = self.effective_permissions(journal_id, authority).await?;
        if !perms.contains(Permissions::READ) {
            return Err(PermissionError(Permissions::READ));
        }

        let child_ids = self.journal_store.get_subjournals(journal_id).await?;
        let mut result = Vec::new();
        for child_id in child_ids {
            if let Some(state) = self.journal_store.get_journal(child_id).await? {
                result.push((child_id, state));
            }
        }
        Ok(result)
    }

    /// Returns all descendants of `journal_id` at any depth (breadth-first), as a flat list.
    /// The list preserves parent-before-child ordering so callers can recurse through it.
    pub async fn get_subjournals(
        &self,
        journal_id: JournalId,
        authority: &Authority,
    ) -> JournalStoreResult<Vec<(JournalId, JournalState)>> {
        let perms = self.effective_permissions(journal_id, authority).await?;
        if !perms.contains(Permissions::READ) {
            return Err(PermissionError(Permissions::READ));
        }

        let mut result = Vec::new();
        let mut queue = vec![journal_id];

        while let Some(current_id) = queue.pop() {
            let direct_child_ids = self.journal_store.get_subjournals(current_id).await?;
            for child_id in direct_child_ids {
                if let Some(state) = self.journal_store.get_journal(child_id).await? {
                    result.push((child_id, state));
                    queue.push(child_id);
                }
            }
        }

        Ok(result)
    }

    pub async fn get_name(&self, journal_id: JournalId) -> JournalStoreResult<Option<Name>> {
        self.journal_store.get_name(journal_id).await
    }

    /// Returns the name path from `journal_id` up to (but not including) `stop_at_ancestor`,
    /// in ancestor-first order. Returns `None` if any journal in the chain is missing.
    /// Returns an empty vec if `journal_id == stop_at_ancestor`.
    #[expect(dead_code)]
    pub async fn journal_get_relative_name_path(
        &self,
        journal_id: JournalId,
        stop_at_ancestor: JournalId,
    ) -> JournalStoreResult<Option<Vec<Name>>> {
        if journal_id == stop_at_ancestor {
            return Ok(Some(vec![]));
        }

        let mut parts = Vec::new();
        let mut current_id = journal_id;
        loop {
            let state = match self.journal_store.get_journal(current_id).await? {
                Some(s) => s,
                None => return Ok(None),
            };
            if current_id == stop_at_ancestor {
                break;
            }
            parts.push(state.name);
            match state.parent_journal_id {
                Some(parent_id) => current_id = parent_id,
                None => break,
            }
        }
        parts.reverse();
        Ok(Some(parts))
    }

    pub async fn get_name_from_res<E>(
        &self,
        journal_id_res: Result<JournalId, E>,
    ) -> JournalStoreResult<Option<Name>>
    where
        JournalStoreError: From<E>,
    {
        self.get_name(journal_id_res?).await
    }

    pub async fn journal_invite_member(
        &self,
        journal_id: JournalId,
        authority: &Authority,
        invitee: Email,
        permissions: Permissions,
    ) -> JournalStoreResult<u64> {
        let journal_state = self
            .journal_store
            .get_journal(journal_id)
            .await?
            .ok_or(InvalidJournal(journal_id))?;

        if journal_state.deleted {
            return Err(InvalidJournal(journal_id));
        }

        let invitee_id = self
            .user_store
            .lookup_user_id(invitee.as_ref())
            .await?
            .ok_or(UserLookupFailed(invitee.clone()))?;

        if journal_state.members.contains_key(&invitee_id) {
            return Err(UserAlreadyHasAccess(invitee));
        }

        if !journal_state
            .get_actor_permissions(authority)
            .contains(Permissions::INVITE)
        {
            return Err(PermissionError(Permissions::INVITE));
        }

        self.journal_store
            .record(
                journal_id,
                authority.clone(),
                JounalPayload::AddedTenant {
                    id: invitee_id,
                    permissions,
                },
            )
            .await
    }

    pub async fn update_member_permissions(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        permissions: Permissions,
        authority: Authority,
    ) -> JournalStoreResult<u64> {
        let journal_state = self
            .journal_store
            .get_journal(journal_id)
            .await?
            .ok_or(InvalidJournal(journal_id))?;

        if journal_state.deleted {
            return Err(InvalidJournal(journal_id));
        }

        if !journal_state.members.contains_key(&target_user) {
            return Err(UserDoesntHaveAccess);
        }

        if !journal_state
            .get_actor_permissions(&authority)
            .contains(Permissions::OWNER)
        {
            return Err(PermissionError(Permissions::OWNER));
        }

        self.journal_store
            .record(
                journal_id,
                authority,
                JounalPayload::UpdatedTenantPermissions {
                    id: target_user,
                    permissions,
                },
            )
            .await
    }

    pub async fn remove_member(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        authority: Authority,
    ) -> JournalStoreResult<u64> {
        let journal_state = self
            .journal_store
            .get_journal(journal_id)
            .await?
            .ok_or(InvalidJournal(journal_id))?;

        if journal_state.deleted {
            return Err(InvalidJournal(journal_id));
        }

        if !journal_state.members.contains_key(&target_user) {
            return Err(InvalidUser(target_user));
        }

        if !journal_state
            .get_actor_permissions(&authority)
            .contains(Permissions::OWNER)
        {
            return Err(PermissionError(Permissions::OWNER));
        }

        self.journal_store
            .record(
                journal_id,
                authority,
                JounalPayload::RemovedTenant { id: target_user },
            )
            .await
    }

    pub async fn get_creation_timestamp(
        &self,
        journal_id: JournalId,
    ) -> JournalStoreResult<Option<DateTime<Utc>>> {
        self.journal_store.get_creation_timestamp(journal_id).await
    }

    pub async fn get_creator(
        &self,
        journal_id: JournalId,
    ) -> JournalStoreResult<Option<Authority>> {
        self.journal_store.get_creator(journal_id).await
    }
}
