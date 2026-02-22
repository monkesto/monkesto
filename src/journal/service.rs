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
    pub(crate) fn new(journal_store: J, user_store: U) -> Self {
        Self {
            journal_store,
            user_store,
        }
    }

    pub(crate) async fn journal_create(
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
                },
                None,
            )
            .await
    }

    pub(crate) async fn journal_list(
        &self,
        actor: UserId,
    ) -> Result<Vec<(JournalId, JournalState)>, KnownErrors> {
        let ids = self.journal_store.get_user_journals(actor).await?;

        let mut journals = Vec::new();

        for id in ids {
            journals.push((
                id,
                self.journal_store
                    .get_journal(id)
                    .await?
                    .ok_or(KnownErrors::InvalidJournal)?,
            ))
        }

        Ok(journals)
    }

    pub(crate) async fn journal_get(
        &self,
        journal_id: JournalId,
        actor: UserId,
    ) -> Result<Option<JournalState>, KnownErrors> {
        let state = self.journal_store.get_journal(journal_id).await;

        match state {
            Ok(Some(s)) => {
                if s.get_user_permissions(actor).contains(Permissions::READ) {
                    Ok(Some(s))
                } else {
                    Ok(None)
                }
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    pub(crate) async fn journal_get_users(
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

    pub(crate) async fn journal_get_name(
        &self,
        journal_id: JournalId,
    ) -> Result<Option<String>, KnownErrors> {
        self.journal_store.get_name(journal_id).await
    }

    pub(crate) async fn journal_get_name_from_res<E>(
        &self,
        journal_id_res: Result<JournalId, E>,
    ) -> Result<Option<String>, KnownErrors>
    where
        KnownErrors: From<E>,
    {
        self.journal_get_name(journal_id_res?).await
    }

    pub(crate) async fn journal_invite_tenant(
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

    pub(crate) async fn journal_update_tenant_permissions(
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

    pub(crate) async fn journal_remove_tenant(
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
