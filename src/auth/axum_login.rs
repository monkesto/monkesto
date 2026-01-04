use crate::{cuid::Cuid, known_errors::KnownErrors};
use axum_login::{AuthUser, AuthnBackend, UserId};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, prelude::FromRow};
use std::fmt;
use tokio::task;

#[derive(Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Cuid,
    username: String,
    pw_hash: Vec<u8>,
}

impl fmt::Debug for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("username", &self.username)
            .field("password", &"[redacted]")
            .finish()
    }
}

impl AuthUser for User {
    type Id = Cuid;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        &self.pw_hash
    }
}

#[derive(Clone)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Clone)]
pub struct Backend {
    pub db: PgPool,
}

impl Backend {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }
    pub async fn create_auth_user(
        &self,
        id: Cuid,
        username: String,
        pw_hash: String,
    ) -> Result<(), KnownErrors> {
        sqlx::query(
            r#"
        INSERT INTO auth_users (user_id, username, password)
        VALUES ($1, $2, $3)
        "#,
        )
        .bind(id.to_bytes())
        .bind(username)
        .bind(pw_hash)
        .execute(&self.db)
        .await?;

        Ok(())
    }
}

impl AuthnBackend for Backend {
    type User = User;
    type Credentials = Credentials;
    type Error = KnownErrors;

    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        let user_info: Option<(Vec<u8>, String)> =
            sqlx::query_scalar(r#"SELECT (user_id, password) FROM auth_users WHERE username = $1"#)
                .bind(creds.username.clone())
                .fetch_optional(&self.db)
                .await?;

        if let Some((id, password)) = user_info {
            return task::spawn_blocking(move || {
                if bcrypt::verify(creds.password, &password).is_ok_and(|f| f) {
                    return Ok(Some(User {
                        id: Cuid::from_bytes(&id)?,
                        username: creds.username,
                        pw_hash: password.into_bytes(),
                    }));
                }

                Ok(None)
            })
            .await?;
        }

        Ok(None)
    }

    async fn get_user(&self, user_id: &UserId<Self>) -> Result<Option<Self::User>, Self::Error> {
        let user_info: Option<(String, String)> =
            sqlx::query_scalar(r#"SELECT (username, password) FROM auth_users WHERE user_id = $1"#)
                .bind(user_id.to_bytes())
                .fetch_optional(&self.db)
                .await?;
        if let Some((username, pw_hash)) = user_info {
            Ok(Some(User {
                id: *user_id,
                username,
                pw_hash: pw_hash.into_bytes(),
            }))
        } else {
            Ok(None)
        }
    }
}

pub type AuthSession = axum_login::AuthSession<Backend>;
