use crate::auth::user::Email;
use crate::auth::user::UserStore;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::authority::UserId;
use crate::ident::JournalId;
use crate::journal::JournalEvent;
use crate::journal::JournalState;
use crate::journal::JournalStore;
use crate::journal::JournalTenantInfo;
use crate::journal::Permissions;
use crate::known_errors::KnownErrors;
use crate::known_errors::KnownErrors::PermissionError;
use chrono::Utc;

#[derive(Clone)]
pub struct JournalService<J, U>
where
    J: JournalStore,
    U: UserStore,
{
    journal_store: J,
    user_store: U,
}

impl<J, U> JournalService<J, U>
where
    J: JournalStore,
    U: UserStore,
{
    pub fn new(journal_store: J, user_store: U) -> Self {
        Self {
            journal_store,
            user_store,
        }
    }

    pub async fn journal_create(
        &self,
        journal_id: JournalId,
        name: String,
        actor: UserId,
    ) -> Result<(), KnownErrors> {
        self.journal_store
            .record(
                journal_id,
                Authority::Direct(Actor::Anonymous),
                JournalEvent::Created {
                    name,
                    creator: actor,
                    created_at: Utc::now(),
                    parent_journal_id: None,
                },
                None,
            )
            .await
    }

    pub async fn journal_create_subjournal(
        &self,
        parent_journal_id: JournalId,
        name: String,
        actor: UserId,
    ) -> Result<JournalId, KnownErrors> {
        let parent_state = self
            .journal_store
            .get_journal(parent_journal_id)
            .await?
            .ok_or(KnownErrors::InvalidJournal)?;

        if parent_state.deleted {
            return Err(KnownErrors::InvalidJournal);
        }

        if !parent_state
            .get_user_permissions(actor)
            .contains(Permissions::ADDACCOUNT)
        {
            return Err(PermissionError {
                required_permissions: Permissions::ADDACCOUNT,
            });
        }

        let subjournal_id = JournalId::new();
        self.journal_store
            .record(
                subjournal_id,
                Authority::Direct(Actor::Anonymous),
                JournalEvent::Created {
                    name,
                    creator: actor,
                    created_at: Utc::now(),
                    parent_journal_id: Some(parent_journal_id),
                },
                None,
            )
            .await?;
        Ok(subjournal_id)
    }

    /// Returns `journal_id` and all its ancestors up to (and including) the root, in
    /// child-first order (i.e. the given journal comes first, root comes last).
    pub async fn journal_get_ancestor_ids(
        &self,
        journal_id: JournalId,
    ) -> Result<Vec<JournalId>, KnownErrors> {
        let mut chain = Vec::new();
        let mut current_id = journal_id;
        loop {
            chain.push(current_id);
            let state = self
                .journal_store
                .get_journal(current_id)
                .await?
                .ok_or(KnownErrors::InvalidJournal)?;
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
        actor: UserId,
    ) -> Result<Permissions, KnownErrors> {
        let mut current_id = journal_id;
        loop {
            let state = self
                .journal_store
                .get_journal(current_id)
                .await?
                .ok_or(KnownErrors::InvalidJournal)?;

            let perms = state.get_user_permissions(actor);
            if !perms.is_empty() {
                return Ok(perms);
            }

            match state.parent_journal_id {
                Some(parent_id) => current_id = parent_id,
                None => return Ok(Permissions::empty()),
            }
        }
    }

    pub async fn journal_list(
        &self,
        actor: UserId,
    ) -> Result<Vec<(JournalId, JournalState)>, KnownErrors> {
        let ids = self.journal_store.get_user_journals(actor).await?;

        let mut journals = Vec::new();

        for id in ids {
            let state = self
                .journal_store
                .get_journal(id)
                .await?
                .ok_or(KnownErrors::InvalidJournal)?;

            // Only show top-level journals; subjournals are accessed through their parent
            if state.parent_journal_id.is_none() {
                journals.push((id, state));
            }
        }

        Ok(journals)
    }

    pub async fn journal_get(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Option<JournalState>, KnownErrors> {
        let state = match self.journal_store.get_journal(journal_id).await? {
            Some(s) => s,
            None => return Ok(None),
        };

        let perms = self.effective_permissions(journal_id, actor).await?;
        if perms.contains(Permissions::READ) {
            Ok(Some(state))
        } else {
            Ok(None)
        }
    }

    pub async fn journal_get_users(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Vec<UserId>, KnownErrors> {
        let journal_state = self
            .journal_store
            .get_journal(journal_id)
            .await?
            .ok_or(KnownErrors::InvalidJournal)?;

        if !journal_state
            .get_user_permissions(actor)
            .contains(Permissions::READ)
        {
            return Err(PermissionError {
                required_permissions: Permissions::READ,
            });
        }

        let mut users = Vec::new();

        for (user_id, _) in journal_state.tenants.iter() {
            users.push(*user_id);
        }

        users.push(journal_state.creator);

        Ok(users)
    }

    /// Returns only the direct children of `journal_id` (depth 1).
    pub async fn journal_get_direct_subjournals(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Vec<(JournalId, JournalState)>, KnownErrors> {
        let perms = self.effective_permissions(journal_id, actor).await?;
        if !perms.contains(Permissions::READ) {
            return Err(PermissionError {
                required_permissions: Permissions::READ,
            });
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
    pub async fn journal_get_subjournals(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Vec<(JournalId, JournalState)>, KnownErrors> {
        let perms = self.effective_permissions(journal_id, actor).await?;
        if !perms.contains(Permissions::READ) {
            return Err(PermissionError {
                required_permissions: Permissions::READ,
            });
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

    pub async fn journal_get_name(
        &self,
        journal_id: JournalId,
    ) -> Result<Option<String>, KnownErrors> {
        self.journal_store.get_name(journal_id).await
    }

    /// Returns the name path from `journal_id` up to (but not including) `stop_at_ancestor`,
    /// in ancestor-first order. Returns `None` if any journal in the chain is missing.
    /// Returns an empty vec if `journal_id == stop_at_ancestor`.
    pub async fn journal_get_relative_name_path(
        &self,
        journal_id: JournalId,
        stop_at_ancestor: JournalId,
    ) -> Result<Option<Vec<String>>, KnownErrors> {
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

    pub async fn journal_get_name_from_res<E>(
        &self,
        journal_id_res: Result<JournalId, E>,
    ) -> Result<Option<String>, KnownErrors>
    where
        KnownErrors: From<E>,
    {
        self.journal_get_name(journal_id_res?).await
    }

    pub async fn journal_invite_tenant(
        &self,
        journal_id: JournalId,
        actor: UserId,
        invitee: Email,
        permissions: Permissions,
    ) -> Result<(), KnownErrors> {
        let journal_state = self
            .journal_store
            .get_journal(journal_id)
            .await?
            .ok_or(KnownErrors::InvalidJournal)?;

        if journal_state.deleted {
            return Err(KnownErrors::InvalidJournal);
        }

        let invitee_id = self
            .user_store
            .lookup_user_id(invitee.as_ref())
            .await
            .map_err(|e| KnownErrors::InternalError {
                context: e.to_string(),
            })?
            .ok_or(KnownErrors::UserDoesntExist)?;

        if journal_state.tenants.contains_key(&invitee_id) {
            return Err(KnownErrors::UserCanAccessJournal);
        }

        if !journal_state
            .get_user_permissions(actor)
            .contains(Permissions::INVITE)
        {
            return Err(PermissionError {
                required_permissions: Permissions::INVITE,
            });
        }

        let tenant_info = JournalTenantInfo {
            tenant_permissions: permissions,
            inviting_user: actor,
            invited_at: Utc::now(),
        };

        self.journal_store
            .record(
                journal_id,
                Authority::Direct(Actor::Anonymous),
                JournalEvent::AddedTenant {
                    id: invitee_id,
                    tenant_info,
                },
                None,
            )
            .await
    }

    pub async fn journal_update_tenant_permissions(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        permissions: Permissions,
        actor: UserId,
    ) -> Result<(), KnownErrors> {
        let journal_state = self
            .journal_store
            .get_journal(journal_id)
            .await?
            .ok_or(KnownErrors::InvalidJournal)?;

        if journal_state.deleted {
            return Err(KnownErrors::InvalidJournal);
        }

        if !journal_state.tenants.contains_key(&target_user) {
            return Err(KnownErrors::UserDoesntExist);
        }

        if !journal_state
            .get_user_permissions(actor)
            .contains(Permissions::OWNER)
        {
            return Err(PermissionError {
                required_permissions: Permissions::OWNER,
            });
        }

        self.journal_store
            .record(
                journal_id,
                Authority::Direct(Actor::Anonymous),
                JournalEvent::UpdatedTenantPermissions {
                    id: target_user,
                    permissions,
                },
                None,
            )
            .await
    }

    pub async fn journal_remove_tenant(
        &self,
        journal_id: JournalId,
        target_user: UserId,
        actor: UserId,
    ) -> Result<(), KnownErrors> {
        let journal_state = self
            .journal_store
            .get_journal(journal_id)
            .await?
            .ok_or(KnownErrors::InvalidJournal)?;

        if journal_state.deleted {
            return Err(KnownErrors::InvalidJournal);
        }

        if !journal_state.tenants.contains_key(&target_user) {
            return Err(KnownErrors::UserDoesntExist);
        }

        if !journal_state
            .get_user_permissions(actor)
            .contains(Permissions::OWNER)
        {
            return Err(PermissionError {
                required_permissions: Permissions::OWNER,
            });
        }

        self.journal_store
            .record(
                journal_id,
                Authority::Direct(Actor::Anonymous),
                JournalEvent::RemovedTenant { id: target_user },
                None,
            )
            .await
    }
}
