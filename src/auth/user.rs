use crate::cuid::Cuid;
use crate::journal::JournalTenantInfo;
use crate::journal::Permissions;
use crate::known_errors::KnownErrors;
use postcard::{from_bytes, to_allocvec};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, query_as};
use std::collections::{HashMap, HashSet};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum UserEvent {
    Created {
        hashed_password: String,
    },
    PasswordUpdated {
        hashed_password: String,
    },
    CreatedJournal {
        id: Cuid,
    },
    InvitedToJournal {
        id: Cuid,
        permissions: Permissions,
        inviting_user: Cuid,
        owner: Cuid,
    },
    AcceptedJournalInvite {
        id: Cuid,
    },
    DeclinedJournalInvite {
        id: Cuid,
    },
    RemovedFromJournal {
        id: Cuid,
    },
    SelectedJournal {
        id: Cuid,
    },
    Deleted,
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "smallint")]
#[repr(i16)]
pub enum UserEventType {
    Created = 1,
    UsernameUpdated = 2,
    PasswordUpdated = 3,
    CreatedJournal = 4,
    InvitedToJournal = 5,
    AcceptedJournalInvite = 6,
    DeclinedJournalInvite = 7,
    RemovedFromJournal = 8,
    SelectedJournal = 9,
    Deleted = 10,
}

impl UserEvent {
    pub fn get_type(&self) -> UserEventType {
        use UserEventType::*;
        match self {
            Self::Created { .. } => Created,
            Self::PasswordUpdated { .. } => PasswordUpdated,
            Self::CreatedJournal { .. } => CreatedJournal,
            Self::InvitedToJournal { .. } => InvitedToJournal,
            Self::AcceptedJournalInvite { .. } => AcceptedJournalInvite,
            Self::DeclinedJournalInvite { .. } => DeclinedJournalInvite,
            Self::RemovedFromJournal { .. } => RemovedFromJournal,
            Self::SelectedJournal { .. } => SelectedJournal,
            Self::Deleted => Deleted,
        }
    }
    pub async fn push_db(&self, id: &Cuid, pool: &PgPool) -> Result<i64, KnownErrors> {
        let event_type = self.get_type();
        let payload: Vec<u8> = to_allocvec(self)?;

        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO user_events (
                user_id,
                event_type,
                payload
            )
            VALUES ($1, $2, $3)
            RETURNING id
            "#,
        )
        .bind(id.to_bytes())
        .bind(event_type)
        .bind(payload)
        .fetch_one(pool)
        .await?;

        Ok(id)
    }
}

#[allow(dead_code)]
#[derive(Default)]
pub struct UserState {
    pub id: Cuid,
    pub authenticated_sessions: std::collections::HashSet<String>,
    pub hashed_password: String,
    pub pending_journal_invites: HashMap<Cuid, JournalTenantInfo>,
    pub accepted_journal_invites: HashMap<Cuid, JournalTenantInfo>,
    pub owned_journals: HashSet<Cuid>,
    pub selected_journal: Cuid,
    pub deleted: bool,
}

impl UserState {
    pub async fn build(
        id: &Cuid,
        event_types: Vec<UserEventType>,
        pool: &PgPool,
    ) -> Result<Self, KnownErrors> {
        let user_events = query_as::<_, (Vec<u8>,)>(
            r#"
            SELECT payload FROM user_events
            WHERE user_id = $1 AND event_type = ANY($2)
            ORDER BY created_at ASC
            "#,
        )
        .bind(id.to_bytes())
        .bind(&event_types)
        .fetch_all(pool)
        .await?;

        let mut aggregate = Self {
            id: *id,
            selected_journal: Cuid::default(),
            ..Default::default()
        };

        user_events
            .into_iter()
            .try_for_each(|(payload,)| -> Result<(), KnownErrors> {
                aggregate.apply(from_bytes::<UserEvent>(&payload)?);
                Ok(())
            })?;

        Ok(aggregate)
    }

    pub fn apply(&mut self, event: UserEvent) {
        match event {
            UserEvent::Created {
                hashed_password: password,
            } => {
                self.hashed_password = password;
            }
            UserEvent::PasswordUpdated {
                hashed_password: password,
            } => self.hashed_password = password,
            UserEvent::CreatedJournal { id } => _ = self.owned_journals.insert(id),
            UserEvent::InvitedToJournal {
                id,
                permissions,
                inviting_user,
                owner,
            } => {
                _ = self.pending_journal_invites.insert(
                    id,
                    JournalTenantInfo {
                        tenant_permissions: permissions,
                        inviting_user,
                        journal_owner: owner,
                    },
                )
            }
            UserEvent::DeclinedJournalInvite { id } => _ = self.pending_journal_invites.remove(&id),
            UserEvent::AcceptedJournalInvite { id } => {
                let tenant_info = self.pending_journal_invites.remove(&id);

                if let Some(unwrapped_tenant_info) = tenant_info {
                    _ = self
                        .accepted_journal_invites
                        .insert(id, unwrapped_tenant_info);
                }
            }
            UserEvent::RemovedFromJournal { id } => _ = self.accepted_journal_invites.remove(&id),
            UserEvent::SelectedJournal { id } => self.selected_journal = id,
            UserEvent::Deleted => self.deleted = true,
        }
    }
}

pub async fn get_hashed_pw(user_id: &Cuid, pool: &PgPool) -> Result<String, KnownErrors> {
    let user = UserState::build(
        user_id,
        vec![UserEventType::Created, UserEventType::PasswordUpdated],
        pool,
    )
    .await?;

    Ok(user.hashed_password)
}
