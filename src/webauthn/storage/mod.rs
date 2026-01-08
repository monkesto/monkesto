use webauthn_rs::prelude::{Passkey, Uuid};

pub mod database;
pub mod memory;

/// Errors that can occur during storage operations
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("User not found")]
    UserNotFound,
    #[error("Email already exists")]
    EmailAlreadyExists,
    #[error("Storage operation failed: {0}")]
    #[allow(dead_code)]
    OperationFailed(String),
}

/// Trait for abstracting user and passkey storage operations
#[async_trait::async_trait]
pub trait UserStorage: Send + Sync {
    /// Check if an email already exists in the system
    async fn email_exists(&self, email: &str) -> Result<bool, StorageError>;

    /// Get the email address for a specific user ID
    async fn get_user_email(&self, user_id: &Uuid) -> Result<String, StorageError>;

    /// Create a new user with their first passkey
    async fn create_user(
        &self,
        email: String,
        user_id: Uuid,
        passkey: Passkey,
    ) -> Result<(), StorageError>;

    /// Get all passkeys for a specific user
    async fn get_user_passkeys(&self, user_id: &Uuid) -> Result<Vec<Passkey>, StorageError>;

    /// Add a new passkey to an existing user
    async fn add_passkey(&self, user_id: &Uuid, passkey: Passkey) -> Result<(), StorageError>;

    /// Remove a specific passkey from a user
    async fn remove_passkey(&self, user_id: &Uuid, passkey_id: &[u8])
    -> Result<bool, StorageError>;

    /// Get all credentials from all users (for usernameless authentication)
    async fn get_all_credentials(&self) -> Result<Vec<Passkey>, StorageError>;

    /// Find user ID by passkey credential ID
    async fn find_user_by_credential(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<Uuid>, StorageError>;
}
