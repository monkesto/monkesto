use crate::{
    cuid::Cuid,
    known_errors::{KnownErrors, MonkestoResult},
    webauthn::user::Email,
};
use axum_login::{AuthUser, AuthnBackend, UserId};
use dashmap::DashMap;
use rand::{TryRngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;
use std::{fmt, sync::Arc};
use tokio::task::{self, spawn_blocking};

#[derive(Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Cuid,
    email: Email,
    pw_hash: String,
    session_hash: [u8; 16],
}

impl fmt::Debug for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("email", &self.email)
            .field("password", &"[redacted]")
            .finish()
    }
}

impl User {
    pub async fn new(email: Email, password: String) -> MonkestoResult<Self> {
        let mut session_hash = [0u8; 16];
        OsRng
            .try_fill_bytes(&mut session_hash)
            .map_err(|e| KnownErrors::OsError {
                context: e.to_string(),
            })?;

        Ok(Self {
            id: Cuid::new10(),
            email,
            pw_hash: spawn_blocking(move || {
                bcrypt::hash(password, bcrypt::DEFAULT_COST)
                    .expect("bcrypt password hashing failed")
            })
            .await?,
            session_hash,
        })
    }
}

impl AuthUser for User {
    type Id = Cuid;

    fn id(&self) -> Cuid {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        &self.session_hash
    }
}

#[derive(Clone)]
pub struct Credentials {
    pub email: Email,
    pub password: String,
}

#[derive(Clone)]
pub struct MemoryBackend {
    pub users: Arc<DashMap<Cuid, User>>,
    pub email_lookup_table: Arc<DashMap<Email, Cuid>>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            users: Arc::new(DashMap::new()),
            email_lookup_table: Arc::new(DashMap::new()),
        }
    }

    pub fn add_user(&mut self, user: User) {
        self.email_lookup_table.insert(user.email.clone(), user.id);
        self.users.insert(user.id, user);
    }
}

impl AuthnBackend for MemoryBackend {
    type User = User;
    type Credentials = Credentials;
    type Error = KnownErrors;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, KnownErrors> {
        if let Some(id) = self.email_lookup_table.get(&creds.email)
            && let Some(state) = self.users.get(&id)
        {
            let state_clone = state.value().clone();

            return task::spawn_blocking(move || {
                if bcrypt::verify(creds.password, &state_clone.pw_hash).is_ok_and(|p| p) {
                    Ok(Some(state_clone))
                } else {
                    Ok(None)
                }
            })
            .await?;
        }
        Ok(None)
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, KnownErrors> {
        Ok(self.users.get(user_id).map(|s| (*s).clone()))
    }
}

pub type AuthSession = axum_login::AuthSession<MemoryBackend>;
