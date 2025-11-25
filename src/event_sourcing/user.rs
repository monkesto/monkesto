use super::journal::JournalTenantInfo;
use leptos::prelude::ServerFnError;
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, query_scalar, types::JsonValue};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use crate::{event_sourcing::journal::Permissions, main_api::return_types::KnownErrors};

use super::event::{AggregateType, DomainEvent, EventType};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type", content = "data")]
pub enum UserEvent {
    Created {
        username: String,
        hashed_password: String,
    },
    UsernameUpdated {
        username: String,
    },
    PasswordUpdated {
        hashed_password: String,
    },
    LoggedIn {
        session_id: String,
    },
    LoggedOut {
        session_id: String,
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

impl UserEvent {
    pub async fn push_db(&self, uuid: &Uuid, pool: &PgPool) -> Result<i64, ServerFnError> {
        let payload = serde_json::to_value(DomainEvent::User(self.clone()))?;
        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO events (
                aggregate_id,
                aggregate_type,
                event_type,
                payload
            )
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(uuid)
        .bind(AggregateType::User)
        .bind(EventType::from_user_event(self))
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
    pub username: String,
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
        event_types: Vec<EventType>,
        pool: &PgPool,
    ) -> Result<Self, ServerFnError> {
        let user_events: Vec<JsonValue> = query_scalar(
            r#"
            SELECT payload FROM events
            WHERE aggregate_id = $1 AND aggregate_type = $2 AND event_type = ANY($3)
            ORDER BY created_at ASC
            "#,
        )
        .bind(id)
        .bind(AggregateType::User)
        .bind(&event_types)
        .fetch_all(pool)
        .await?;

        let mut aggregate = Self {
            id: *id,
            selected_journal: Uuid::nil(),
            ..Default::default()
        };

        for raw_event in user_events {
            let domain_event: DomainEvent = serde_json::from_value(raw_event)?;
            aggregate.apply(domain_event.to_user_event()?).await;
        }
        Ok(aggregate)
    }

    pub async fn from_events(id: Uuid, events: Vec<JsonValue>) -> Result<Self, ServerFnError> {
        let mut aggregate = Self {
            id,
            ..Default::default()
        };

        for raw_event in events {
            let domain_event: DomainEvent = serde_json::from_value(raw_event)?;
            aggregate.apply(domain_event.to_user_event()?).await;
        }
        Ok(aggregate)
    }

    pub async fn apply(&mut self, event: UserEvent) {
        match event {
            UserEvent::Created {
                username,
                hashed_password: password,
            } => {
                self.username = username;
                self.hashed_password = password;
            }
            UserEvent::UsernameUpdated { username } => self.username = username,
            UserEvent::PasswordUpdated {
                hashed_password: password,
            } => self.hashed_password = password,
            UserEvent::LoggedIn { session_id } => {
                _ = self.authenticated_sessions.insert(session_id)
            }
            UserEvent::LoggedOut { session_id } => {
                _ = self.authenticated_sessions.remove(&session_id)
            }
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

pub async fn get_id_from_username(
    username: &String,
    pool: &PgPool,
) -> Result<Option<Uuid>, ServerFnError> {
    Ok(query_scalar(
        r#"
        SELECT aggregate_id FROM events
        WHERE (event_type = $1 OR event_type = $2) AND payload->'data'->>'username' = $3
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(EventType::UserCreated)
    .bind(EventType::UsernameUpdated)
    .bind(username)
    .fetch_optional(pool)
    .await?)
}

pub async fn get_id_from_session(
    session_id: &String,
    pool: &PgPool,
) -> Result<Uuid, ServerFnError> {
    let uuid: Option<Uuid> = query_scalar(
        r#"
        SELECT aggregate_id FROM events
        WHERE event_type = $1 AND payload->'data'->>'session_id' = $2
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(EventType::UserLoggedIn)
    .bind(session_id)
    .fetch_optional(pool)
    .await?;

    if let Some(unwrapped_uuid) = uuid {
        let user = UserState::build(
            &unwrapped_uuid,
            vec![
                EventType::UserCreated,
                EventType::UserLoggedIn,
                EventType::UserLoggedOut,
            ],
            pool,
        )
        .await?;

        if user.authenticated_sessions.contains(session_id) {
            return Ok(user.id);
        }
    }

    Err(ServerFnError::ServerError(
        KnownErrors::NotLoggedIn.to_string(),
    ))
}

pub async fn get_username_from_id(user_id: &Uuid, pool: &PgPool) -> Result<String, ServerFnError> {
    let user = UserState::build(
        user_id,
        vec![EventType::UserCreated, EventType::UsernameUpdated],
        pool,
    )
    .await?;

    if !user.username.is_empty() {
        return Ok(user.username);
    }

    Err(ServerFnError::ServerError(
        "unable to fetch username from uuid".to_string(),
    ))
}

pub async fn get_hashed_pw(user_id: &Uuid, pool: &PgPool) -> Result<String, ServerFnError> {
    let user = UserState::build(
        user_id,
        vec![EventType::UserCreated, EventType::UserPasswordUpdated],
        pool,
    )
    .await?;

    Ok(user.hashed_password)
}
