use super::{StorageError, UserStore};
use crate::webauthn::user::UserId;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use webauthn_rs::prelude::Uuid;

pub struct UserData {
    pub email_to_user_id: HashMap<String, UserId>,
    pub user_id_to_webauthn_uuid: HashMap<UserId, Uuid>,
    pub webauthn_uuid_to_user_id: HashMap<Uuid, UserId>,
}

impl UserData {
    pub fn new() -> Self {
        Self {
            email_to_user_id: HashMap::new(),
            user_id_to_webauthn_uuid: HashMap::new(),
            webauthn_uuid_to_user_id: HashMap::new(),
        }
    }
}

impl Default for UserData {
    fn default() -> Self {
        Self::new()
    }
}

/// In-memory storage implementation for users using HashMap
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
    async fn email_exists(&self, email: &str) -> Result<bool, StorageError> {
        let data = self.data.lock().await;
        Ok(data.email_to_user_id.contains_key(email))
    }

    async fn get_user_email(&self, user_id: &UserId) -> Result<String, StorageError> {
        let data = self.data.lock().await;
        data.email_to_user_id
            .iter()
            .find_map(|(email, id)| {
                if id == user_id {
                    Some(email.clone())
                } else {
                    None
                }
            })
            .ok_or(StorageError::UserNotFound)
    }

    async fn get_webauthn_uuid(&self, user_id: &UserId) -> Result<Uuid, StorageError> {
        let data = self.data.lock().await;
        data.user_id_to_webauthn_uuid
            .get(user_id)
            .copied()
            .ok_or(StorageError::UserNotFound)
    }

    async fn create_user(
        &self,
        user_id: UserId,
        webauthn_uuid: Uuid,
        email: String,
    ) -> Result<(), StorageError> {
        let mut data = self.data.lock().await;

        // Check if email already exists
        if data.email_to_user_id.contains_key(&email) {
            return Err(StorageError::EmailAlreadyExists);
        }

        // Insert the new user with both identifiers
        data.email_to_user_id.insert(email, user_id);
        data.user_id_to_webauthn_uuid.insert(user_id, webauthn_uuid);
        data.webauthn_uuid_to_user_id.insert(webauthn_uuid, user_id);

        Ok(())
    }
}
