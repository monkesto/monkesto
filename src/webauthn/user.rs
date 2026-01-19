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

#[expect(dead_code)]
#[derive(Debug, Clone)]
pub struct User {
    id: UserId,
    email: Email,
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

#[expect(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserEvent {
    Created {
        id: UserId,
        by: Authority,
        email: Email,
    },
    Deleted {
        id: UserId,
        by: Authority,
    },
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

    // async fn record(event: UserEvent) -> Result<Self::EventId, Self::Error>;

    /// Check if an email already exists in the system
    async fn email_exists(&self, email: &str) -> Result<bool, Self::Error>;

    /// Get the email address for a specific UserId
    async fn get_user_email(&self, user_id: &UserId) -> Result<String, Self::Error>;

    /// Get the webauthn UUID for a specific UserId
    async fn get_webauthn_uuid(&self, user_id: &UserId) -> Result<Uuid, Self::Error>;

    /// Create a new user
    async fn create_user(
        &self,
        user_id: UserId,
        webauthn_uuid: Uuid,
        email: String,
    ) -> Result<(), Self::Error>;
}
