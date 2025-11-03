use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub(crate) enum UserEvent {
    Created { username: String, password: String },
    UsernameUpdate { username: String },
    PasswordUpdate { password: String },
    Login { session_id: String },
    Logout { session_id: String },
    AddAccount { id: Uuid },
    Deleted,
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
    pub fn from_events(id: Uuid, events: Vec<UserEvent>) -> Self {
        let mut aggregate = Self {
            id,
            ..Default::default()
        };

        for event in events {
            aggregate.apply(event);
        }
        aggregate
    }

    pub fn apply(&mut self, event: UserEvent) {
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
