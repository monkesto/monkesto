use webauthn_rs::prelude::Uuid;

use super::passkey::PasskeyId;
use super::user::UserId;

pub mod memory_passkey;
pub mod memory_user;

#[derive(Debug, Clone)]
pub struct Passkey {
    pub id: PasskeyId,
    pub passkey: webauthn_rs::prelude::Passkey,
}

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

/// Trait for user storage operations
#[async_trait::async_trait]
pub trait UserStore: Send + Sync {
    /// Check if an email already exists in the system
    async fn email_exists(&self, email: &str) -> Result<bool, StorageError>;

    /// Get the email address for a specific UserId
    async fn get_user_email(&self, user_id: &UserId) -> Result<String, StorageError>;

    /// Get the webauthn UUID for a specific UserId
    async fn get_webauthn_uuid(&self, user_id: &UserId) -> Result<Uuid, StorageError>;

    /// Create a new user
    async fn create_user(
        &self,
        user_id: UserId,
        webauthn_uuid: Uuid,
        email: String,
    ) -> Result<(), StorageError>;
}

/// Trait for passkey storage operations
#[async_trait::async_trait]
pub trait PasskeyStore: Send + Sync {
    /// Get all passkeys for a specific user
    async fn get_user_passkeys(&self, user_id: &UserId) -> Result<Vec<Passkey>, StorageError>;

    /// Add a new passkey to an existing user
    async fn add_passkey(
        &self,
        user_id: &UserId,
        passkey_id: PasskeyId,
        passkey: webauthn_rs::prelude::Passkey,
    ) -> Result<(), StorageError>;

    /// Remove a specific passkey from a user by PasskeyId
    async fn remove_passkey(
        &self,
        user_id: &UserId,
        passkey_id: &PasskeyId,
    ) -> Result<bool, StorageError>;

    /// Get all credentials from all users (for usernameless authentication)
    async fn get_all_credentials(&self)
    -> Result<Vec<webauthn_rs::prelude::Passkey>, StorageError>;

    /// Find UserId and PasskeyId by passkey credential ID
    async fn find_user_by_credential(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<(UserId, PasskeyId)>, StorageError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::webauthn::storage::{
        memory_passkey::MemoryPasskeyStore, memory_user::MemoryUserStore,
    };
    use std::sync::Arc;
    use webauthn_rs::prelude::Uuid;

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
            .create_user(user_id, webauthn_uuid, email.clone())
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
    async fn test_passkey_store_operations() {
        let passkey_store = Arc::new(MemoryPasskeyStore::new());
        let user_id = UserId::new();

        // Initially user should have no passkeys
        assert!(
            passkey_store
                .get_user_passkeys(&user_id)
                .await
                .expect("Should get user passkeys")
                .is_empty()
        );

        // Removing non-existent passkey should return false
        let passkey_id = PasskeyId::new();
        assert!(
            !passkey_store
                .remove_passkey(&user_id, &passkey_id)
                .await
                .expect("Should remove passkey")
        );

        // Test that get_all_credentials works when empty
        assert!(
            passkey_store
                .get_all_credentials()
                .await
                .expect("Should get all credentials")
                .is_empty()
        );

        // Test that find_user_by_credential returns None when empty
        assert!(
            passkey_store
                .find_user_by_credential(&[1, 2, 3, 4])
                .await
                .expect("Should find user by credential")
                .is_none()
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
            .create_user(user_id_1, webauthn_uuid_1, email.clone())
            .await
            .expect("Should create first user successfully");

        // Try to create second user with same email
        let result = user_store
            .create_user(user_id_2, webauthn_uuid_2, email.clone())
            .await;

        match result {
            Err(StorageError::EmailAlreadyExists) => {
                // Expected
            }
            _ => panic!("Should have failed with EmailAlreadyExists"),
        }
    }
}
