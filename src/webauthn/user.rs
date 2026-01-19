use super::authority::Authority;
pub use super::authority::UserId;
use nutype::nutype;

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

#[cfg(test)]
mod tests {
    use super::super::authority::{Actor, Authority};
    use super::*;
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

        // Initially email should not exist
        assert!(
            !user_store
                .email_exists(&email)
                .await
                .expect("Should check email existence")
        );

        // Create a user
        user_store
            .record(UserEvent::Created {
                id: user_id,
                by: Authority::Direct(Actor::Anonymous),
                email: Email::try_new(&email).expect("test email should be valid"),
                webauthn_uuid,
            })
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
                .get_user_email(&user_id)
                .await
                .expect("Should get user email"),
            email
        );
        assert_eq!(
            user_store
                .get_webauthn_uuid(&user_id)
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
            .record(UserEvent::Created {
                id: user_id_1,
                by: Authority::Direct(Actor::Anonymous),
                email: Email::try_new(&email).expect("test email should be valid"),
                webauthn_uuid: webauthn_uuid_1,
            })
            .await
            .expect("Should create first user successfully");

        // Try to create second user with same email
        let result = user_store
            .record(UserEvent::Created {
                id: user_id_2,
                by: Authority::Direct(Actor::Anonymous),
                email: Email::try_new(&email).expect("test email should be valid"),
                webauthn_uuid: webauthn_uuid_2,
            })
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
        id: UserId,
        by: Authority,
        email: Email,
        webauthn_uuid: Uuid,
    },
    #[expect(dead_code)]
    Deleted { id: UserId, by: Authority },
}

use webauthn_rs::prelude::Uuid;

#[derive(Debug, thiserror::Error)]
pub enum UserStoreError {
    #[error("User not found")]
    UserNotFound,
    #[error("Email already exists")]
    EmailAlreadyExists,
    #[error("Storage operation failed: {0}")]
    #[allow(dead_code)]
    OperationFailed(String),
}

#[async_trait::async_trait]
pub trait UserStore: Send + Sync {
    type EventId: Send + Sync + Clone;
    type Error;

    async fn record(&self, event: UserEvent) -> Result<Self::EventId, Self::Error>;

    async fn email_exists(&self, email: &str) -> Result<bool, Self::Error>;

    async fn get_user(&self, user_id: &UserId) -> Result<Option<User>, Self::Error>;

    async fn get_user_email(&self, user_id: &UserId) -> Result<String, Self::Error>;

    async fn get_webauthn_uuid(&self, user_id: &UserId) -> Result<Uuid, Self::Error>;
}

use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

struct UserData {
    email_to_user_id: HashMap<String, UserId>,
    user_id_to_email: HashMap<UserId, Email>,
    user_id_to_webauthn_uuid: HashMap<UserId, Uuid>,
    webauthn_uuid_to_user_id: HashMap<Uuid, UserId>,
}

impl UserData {
    fn new() -> Self {
        Self {
            email_to_user_id: HashMap::new(),
            user_id_to_email: HashMap::new(),
            user_id_to_webauthn_uuid: HashMap::new(),
            webauthn_uuid_to_user_id: HashMap::new(),
        }
    }
}

/// In-memory storage implementation for users using HashMap
#[derive(Clone)]
pub struct MemoryUserStore {
    data: Arc<Mutex<UserData>>,
}

impl MemoryUserStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(UserData::new())),
        }
    }
}

impl Default for MemoryUserStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl UserStore for MemoryUserStore {
    type EventId = ();
    type Error = UserStoreError;

    async fn record(&self, event: UserEvent) -> Result<(), UserStoreError> {
        let mut data = self.data.lock().await;

        match event {
            UserEvent::Created {
                id,
                by: _,
                email,
                webauthn_uuid,
            } => {
                let email_str = email.to_string();
                if data.email_to_user_id.contains_key(&email_str) {
                    return Err(UserStoreError::EmailAlreadyExists);
                }
                data.email_to_user_id.insert(email_str, id);
                data.user_id_to_email.insert(id, email);
                data.user_id_to_webauthn_uuid.insert(id, webauthn_uuid);
                data.webauthn_uuid_to_user_id.insert(webauthn_uuid, id);
            }
            UserEvent::Deleted { id, by: _ } => {
                if let Some(webauthn_uuid) = data.user_id_to_webauthn_uuid.remove(&id) {
                    data.webauthn_uuid_to_user_id.remove(&webauthn_uuid);
                }
                data.user_id_to_email.remove(&id);
                data.email_to_user_id.retain(|_, user_id| user_id != &id);
            }
        }

        Ok(())
    }

    async fn email_exists(&self, email: &str) -> Result<bool, UserStoreError> {
        let data = self.data.lock().await;
        Ok(data.email_to_user_id.contains_key(email))
    }

    async fn get_user(&self, user_id: &UserId) -> Result<Option<User>, UserStoreError> {
        let data = self.data.lock().await;
        Ok(data.user_id_to_email.get(user_id).map(|email| User {
            id: *user_id,
            email: email.clone(),
        }))
    }

    async fn get_user_email(&self, user_id: &UserId) -> Result<String, UserStoreError> {
        let data = self.data.lock().await;
        data.user_id_to_email
            .get(user_id)
            .map(|e| e.to_string())
            .ok_or(UserStoreError::UserNotFound)
    }

    async fn get_webauthn_uuid(&self, user_id: &UserId) -> Result<Uuid, UserStoreError> {
        let data = self.data.lock().await;
        data.user_id_to_webauthn_uuid
            .get(user_id)
            .copied()
            .ok_or(UserStoreError::UserNotFound)
    }
}

/// Dummy credentials type - webauthn authentication happens outside axum_login's flow
#[derive(Clone)]
pub struct WebauthnCredentials;

impl axum_login::AuthnBackend for MemoryUserStore {
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
        UserStore::get_user(self, user_id).await
    }
}
