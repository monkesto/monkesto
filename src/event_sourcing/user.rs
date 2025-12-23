use super::journal::JournalTenantInfo;
use leptos::prelude::ServerFnError;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, query_scalar, types::JsonValue};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::event_sourcing::journal::Permissions;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum UserEvent {
    Created {
        hashed_password: String,
    },
    PasswordUpdated {
        hashed_password: String,
    },
    CreatedJournal {
        id: Uuid,
    },
    InvitedToJournal {
        id: Uuid,
        permissions: Permissions,
        inviting_user: Uuid,
        owner: Uuid,
    },
    AcceptedJournalInvite {
        id: Uuid,
    },
    DeclinedJournalInvite {
        id: Uuid,
    },
    RemovedFromJournal {
        id: Uuid,
    },
    SelectedJournal {
        id: Uuid,
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
    pub async fn push_db(&self, uuid: &Uuid, pool: &PgPool) -> Result<i64, ServerFnError> {
        let payload = serde_json::to_value(self.clone())?;
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
        .bind(uuid)
        .bind(self.get_type())
        .bind(payload)
        .fetch_one(pool)
        .await?;

        Ok(id)
    }
}

#[derive(Default)]
pub struct UserState {
    pub id: Uuid,
    pub authenticated_sessions: std::collections::HashSet<String>,
    pub hashed_password: String,
    pub pending_journal_invites: HashMap<Uuid, JournalTenantInfo>,
    pub accepted_journal_invites: HashMap<Uuid, JournalTenantInfo>,
    pub owned_journals: HashSet<Uuid>,
    pub selected_journal: Uuid,
    pub deleted: bool,
}

impl UserState {
    pub async fn build(
        id: &Uuid,
        event_types: Vec<UserEventType>,
        pool: &PgPool,
    ) -> Result<Self, ServerFnError> {
        let user_events: Vec<JsonValue> = query_scalar(
            r#"
            SELECT payload FROM user_events
            WHERE user_id = $1 AND event_type = ANY($2)
            ORDER BY created_at ASC
            "#,
        )
        .bind(id)
        .bind(&event_types)
        .fetch_all(pool)
        .await?;

        let mut aggregate = Self {
            id: *id,
            selected_journal: Uuid::nil(),
            ..Default::default()
        };

        for raw_event in user_events {
            let event: UserEvent = serde_json::from_value(raw_event)?;
            aggregate.apply(event);
        }
        Ok(aggregate)
    }

    pub async fn from_events(id: Uuid, events: Vec<JsonValue>) -> Result<Self, ServerFnError> {
        let mut aggregate = Self {
            id,
            ..Default::default()
        };

        for raw_event in events {
            let event: UserEvent = serde_json::from_value(raw_event)?;
            aggregate.apply(event);
        }
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

pub async fn get_hashed_pw(user_id: &Uuid, pool: &PgPool) -> Result<String, ServerFnError> {
    let user = UserState::build(
        user_id,
        vec![UserEventType::Created, UserEventType::PasswordUpdated],
        pool,
    )
    .await?;

    Ok(user.hashed_password)
}
