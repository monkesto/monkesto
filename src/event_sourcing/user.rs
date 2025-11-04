use leptos::prelude::ServerFnError;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use super::event::{AggregateType, EventType};

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum UserEvent {
    Created { username: String, password: String },
    UsernameUpdate { username: String },
    PasswordUpdate { password: String },
    Login { session_id: String },
    Logout { session_id: String },
    AddAccount { id: Uuid },
    Deleted,
}

impl UserEvent {
    pub async fn push_db(&self, uuid: Uuid, pool: &PgPool) -> Result<i64, ServerFnError> {
        let payload = match serde_json::to_value(self) {
            Ok(s) => s,
            Err(e) => return Err(ServerFnError::ServerError(e.to_string())),
        };
        match sqlx::query_scalar(
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
        .await
        {
            Ok(s) => Ok(s),
            Err(e) => Err(ServerFnError::ServerError(e.to_string())),
        }
    }
}

#[derive(Default)]
struct UserState {
    id: Uuid,
    authenticated_sessions: std::collections::HashSet<String>,
    username: String,
    password: String,
    accounts: std::collections::HashSet<Uuid>,
    deleted: bool,
}

impl UserState {
    pub async fn from_events(id: Uuid, events: Vec<UserEvent>) -> Self {
        let mut aggregate = Self {
            id,
            ..Default::default()
        };

        for event in events {
            aggregate.apply(event).await;
        }
        aggregate
    }

    pub async fn apply(&mut self, event: UserEvent) {
        match event {
            UserEvent::Created { username, password } => {
                self.username = username;
                self.password = password;
            }
            UserEvent::AddAccount { id } => _ = self.accounts.insert(id),
            UserEvent::UsernameUpdate { username } => self.username = username,
            UserEvent::PasswordUpdate { password } => self.password = password,
            UserEvent::Login { session_id } => _ = self.authenticated_sessions.insert(session_id),
            UserEvent::Logout { session_id } => _ = self.authenticated_sessions.remove(&session_id),
            UserEvent::Deleted => self.deleted = true,
        }
    }
}
