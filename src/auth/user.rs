use crate::authority::Authority;
use crate::event::EventStore;
use crate::id;
use crate::ident::Ident;
use crate::known_errors::KnownErrors;
use crate::known_errors::RedirectOnError;
use axum::response::Redirect;
use nutype::nutype;
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Display;
use std::ops::Deref;
use std::str::FromStr;

// Define UserId here in the user module
id!(UserId, Ident::new16());

#[nutype(
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        Hash,
        Serialize,
        Deserialize,
        AsRef,
        Display,
        TryFrom,
        Default
    ),
    sanitize(trim, lowercase),
    validate(regex = r"^[\w\-\.]+@([\w-]+\.)+[\w-]{2,}$"),
    default = "test@email.com"
)]
pub struct Email(String);

#[derive(Debug, Clone)]
pub struct User {
    pub id: UserId,
    pub email: Email,
}

impl axum_login::AuthUser for User {
    type Id = UserId;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        // We don't invalidate sessions based on credential changes
        &[]
    }
}

pub fn get_user<T>(session: AuthSession<T>) -> Result<User, Redirect>
where
    T: AuthnBackend<User = User>,
{
    session
        .user
        .ok_or(KnownErrors::NotLoggedIn)
        .or_redirect("/signin")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::Actor;
    use crate::authority::Authority;
    use std::sync::Arc;

    #[test]
    fn test_email_validation() {
        // test a basic, valid email
        assert!(Email::try_new("test@example.com").is_ok());

        // test sanitization
        assert!(
            Email::try_new("   test.test2@EXamPle.Com   ")
                .is_ok_and(|f| f.to_string() == "test.test2@example.com")
        );

        // test an email without a TLD
        assert_eq!(
            Email::try_new("test@example"),
            Err(EmailError::RegexViolated)
        );

        // test an email without a name
        assert_eq!(
            Email::try_new("@example.com"),
            Err(EmailError::RegexViolated)
        );

        // test an email without an "@"
        assert_eq!(
            Email::try_new("testexample.com"),
            Err(EmailError::RegexViolated)
        );
    }

    #[tokio::test]
    async fn test_user_store_operations() {
        let user_store = Arc::new(MemoryUserStore::new());

        let user_id = UserId::new();
        let webauthn_uuid = Uuid::new_v4();
        let email = "test@example.com".to_string();

        // Initially, email should not exist
        assert!(
            !user_store
                .email_exists(&email)
                .await
                .expect("Should check email existence")
        );

        // Create a user
        user_store
            .record(
                user_id,
                Authority::Direct(Actor::Anonymous),
                UserEvent::Created {
                    email: Email::try_new(&email).expect("test email should be valid"),
                    webauthn_uuid,
                },
                None,
            )
            .await
            .expect("Should create user successfully");

        // Verify user exists
        assert!(
            user_store
                .email_exists(&email)
                .await
                .expect("Should check email existence")
        );
        assert_eq!(
            user_store
                .get_user_email(user_id)
                .await
                .expect("Should get user email"),
            Some(email)
        );
        assert_eq!(
            user_store
                .get_webauthn_uuid(user_id)
                .await
                .expect("Should get webauthn UUID"),
            webauthn_uuid
        );
    }

    #[tokio::test]
    async fn test_email_already_exists() {
        let user_store = Arc::new(MemoryUserStore::new());

        let user_id_1 = UserId::new();
        let user_id_2 = UserId::new();
        let webauthn_uuid_1 = Uuid::new_v4();
        let webauthn_uuid_2 = Uuid::new_v4();
        let email = "test@example.com".to_string();

        // Create first user
        user_store
            .record(
                user_id_1,
                Authority::Direct(Actor::Anonymous),
                UserEvent::Created {
                    email: Email::try_new(&email).expect("test email should be valid"),
                    webauthn_uuid: webauthn_uuid_1,
                },
                None,
            )
            .await
            .expect("Should create first user successfully");

        // Try to create second user with same email
        let result = user_store
            .record(
                user_id_2,
                Authority::Direct(Actor::Anonymous),
                UserEvent::Created {
                    email: Email::try_new(&email).expect("test email should be valid"),
                    webauthn_uuid: webauthn_uuid_2,
                },
                None,
            )
            .await;

        match result {
            Err(UserStoreError::EmailAlreadyExists) => {
                // Expected
            }
            _ => panic!("Should have failed with EmailAlreadyExists"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserEvent {
    Created {
        email: Email,
        webauthn_uuid: Uuid,
    },
    #[expect(dead_code)]
    Deleted,
}

use webauthn_rs::prelude::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum UserStoreError {
    #[error("User not found")]
    UserNotFound,
    #[error("Email already exists")]
    EmailAlreadyExists,
    #[expect(dead_code)]
    #[error("Storage operation failed: {0}")]
    OperationFailed(String),
}

/// The list of dev user emails (stable across restarts).
pub const DEV_USERS: &[&str] = &["pacioli@monkesto.com", "wedgwood@monkesto.com"];

pub trait UserStore:
    Clone
    + Sync
    + Send
    + EventStore<Id = UserId, Event = UserEvent, Error = UserStoreError>
    + AuthnBackend
{
    async fn email_exists(&self, email: &str) -> Result<bool, <Self as EventStore>::Error>;

    async fn get_user(&self, user_id: UserId) -> Result<Option<User>, <Self as EventStore>::Error>;

    async fn get_user_email(
        &self,
        user_id: UserId,
    ) -> Result<Option<String>, <Self as EventStore>::Error>;

    async fn get_webauthn_uuid(&self, user_id: UserId)
    -> Result<Uuid, <Self as EventStore>::Error>;

    async fn lookup_user_id(
        &self,
        email: &str,
    ) -> Result<Option<UserId>, <Self as EventStore>::Error>;

    /// Seeds two dev users for local development.
    /// Uses stable IDs so sessions remain valid across restarts.
    async fn seed_dev_users(&self) -> Result<(), <Self as EventStore>::Error> {
        use crate::authority::Actor;
        use crate::authority::Authority;
        use std::str::FromStr;

        // Stable IDs for dev users - these are valid cuid2 format (16 chars, lowercase alphanumeric)
        // Generated once and hardcoded to ensure session stability across restarts
        let dev_users = [
            (
                "pacioli@monkesto.com",
                UserId::from_str("zk8m3p5q7r2n4v6x").expect("stable dev user id"),
                Uuid::parse_str("a1b2c3d4-e5f6-4a5b-8c9d-0e1f2a3b4c5d")
                    .expect("stable dev user uuid"),
            ),
            (
                "wedgwood@monkesto.com",
                UserId::from_str("yj7l2o4p6q8s0u1w").expect("stable dev user id"),
                Uuid::parse_str("b2c3d4e5-f6a7-5b6c-9d0e-1f2a3b4c5d6e")
                    .expect("stable dev user uuid"),
            ),
        ];

        for (email, user_id, webauthn_uuid) in dev_users {
            if let Ok(false) = self.email_exists(email).await {
                self.record(
                    user_id,
                    Authority::Direct(Actor::System),
                    UserEvent::Created {
                        email: Email::try_new(email).expect("dev email should be valid"),
                        webauthn_uuid,
                    },
                    None,
                )
                .await?;
            }
        }
        Ok(())
    }

    /// Returns dev users for displaying in the dev login form.
    async fn get_dev_users(&self) -> Vec<User> {
        let mut users = Vec::new();
        for email in DEV_USERS {
            if let Ok(Some(user_id)) = self.lookup_user_id(email).await
                && let Ok(Some(user)) = UserStore::get_user(self, user_id).await
            {
                users.push(user);
            }
        }
        users
    }
}

use axum_login::AuthSession;
use axum_login::AuthnBackend;
use dashmap::DashMap;
use sqlx::PgTransaction;
use std::sync::Arc;

/// In-memory storage implementation for users using HashMap
#[derive(Clone)]
pub struct MemoryUserStore {
    email_to_user_id: Arc<DashMap<String, UserId>>,
    user_id_to_email: Arc<DashMap<UserId, Email>>,
    user_id_to_webauthn_uuid: Arc<DashMap<UserId, Uuid>>,
    webauthn_uuid_to_user_id: Arc<DashMap<Uuid, UserId>>,
}

impl MemoryUserStore {
    pub fn new() -> Self {
        Self {
            email_to_user_id: Arc::new(DashMap::new()),
            user_id_to_email: Arc::new(DashMap::new()),
            user_id_to_webauthn_uuid: Arc::new(DashMap::new()),
            webauthn_uuid_to_user_id: Arc::new(DashMap::new()),
        }
    }
}

impl Default for MemoryUserStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EventStore for MemoryUserStore {
    type Id = UserId;
    type Event = UserEvent;
    type EventId = ();
    type Error = UserStoreError;

    async fn record(
        &self,
        id: UserId,
        _by: Authority,
        event: UserEvent,
        _tx: Option<&mut PgTransaction<'_>>,
    ) -> Result<(), UserStoreError> {
        match event {
            UserEvent::Created {
                email,
                webauthn_uuid,
            } => {
                let email_str = email.to_string();
                if self.email_to_user_id.contains_key(&email_str) {
                    return Err(UserStoreError::EmailAlreadyExists);
                }
                self.email_to_user_id.insert(email_str, id);
                self.user_id_to_email.insert(id, email);
                self.user_id_to_webauthn_uuid.insert(id, webauthn_uuid);
                self.webauthn_uuid_to_user_id.insert(webauthn_uuid, id);
            }
            UserEvent::Deleted => {
                if let Some((_, webauthn_uuid)) = self.user_id_to_webauthn_uuid.remove(&id) {
                    self.webauthn_uuid_to_user_id.remove(&webauthn_uuid);
                }
                self.user_id_to_email.remove(&id);
                self.email_to_user_id.retain(|_, user_id| user_id != &id);
            }
        }

        Ok(())
    }
}

impl UserStore for MemoryUserStore {
    async fn email_exists(&self, email: &str) -> Result<bool, UserStoreError> {
        Ok(self.email_to_user_id.contains_key(email))
    }

    async fn get_user(&self, user_id: UserId) -> Result<Option<User>, UserStoreError> {
        Ok(self.user_id_to_email.get(&user_id).map(|email| User {
            id: user_id,
            email: email.clone(),
        }))
    }

    async fn get_user_email(&self, user_id: UserId) -> Result<Option<String>, UserStoreError> {
        Ok(self.user_id_to_email.get(&user_id).map(|e| e.to_string()))
    }

    async fn get_webauthn_uuid(&self, user_id: UserId) -> Result<Uuid, UserStoreError> {
        self.user_id_to_webauthn_uuid
            .get(&user_id)
            .map(|u| *u)
            .ok_or(UserStoreError::UserNotFound)
    }

    async fn lookup_user_id(&self, email: &str) -> Result<Option<UserId>, UserStoreError> {
        Ok(self.email_to_user_id.get(email).map(|id| *id))
    }
}

/// Dummy credentials type - webauthn authentication happens outside axum_login's flow
#[derive(Clone)]
pub struct WebauthnCredentials;

impl AuthnBackend for MemoryUserStore {
    type User = User;
    type Credentials = WebauthnCredentials;
    type Error = UserStoreError;

    async fn authenticate(
        &self,
        _creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        // Webauthn authentication is handled separately via challenge/response
        // This method is not used - we call session.login() directly after webauthn verification
        Ok(None)
    }

    async fn get_user(&self, user_id: &UserId) -> Result<Option<Self::User>, Self::Error> {
        UserStore::get_user(self, *user_id).await
    }
}

#[derive(Clone)]
pub struct UserService<U: UserStore> {
    user_store: U,
}

impl<U: UserStore> UserService<U> {
    pub fn new(user_store: U) -> Self {
        Self { user_store }
    }

    pub fn store(&self) -> &U {
        &self.user_store
    }

    pub async fn user_get_email(&self, userid: UserId) -> Result<Option<String>, KnownErrors> {
        self.user_store
            .get_user_email(userid)
            .await
            .map_err(|e| KnownErrors::InternalError {
                context: e.to_string(),
            })
    }
}
