use super::{StorageError, WebauthnStorage};
use crate::webauthn::user::UserId;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use webauthn_rs::prelude::{Passkey, Uuid};

/// In-memory data structure for user and passkey storage
pub struct Data {
    /// Maps email to UserId (the actual user identifier)
    pub email_to_user_id: HashMap<String, UserId>,
    /// Maps UserId to webauthn UUID (for webauthn-rs compatibility)
    pub user_id_to_webauthn_uuid: HashMap<UserId, Uuid>,
    /// Maps webauthn UUID to UserId (reverse lookup)
    pub webauthn_uuid_to_user_id: HashMap<Uuid, UserId>,
    /// Maps UserId to passkeys
    pub keys: HashMap<UserId, Vec<Passkey>>,
}

impl Data {
    pub fn new() -> Self {
        Self {
            email_to_user_id: HashMap::new(),
            user_id_to_webauthn_uuid: HashMap::new(),
            webauthn_uuid_to_user_id: HashMap::new(),
            keys: HashMap::new(),
        }
    }
}

impl Default for Data {
    fn default() -> Self {
        Self::new()
    }
}

/// In-memory storage implementation using HashMap
pub struct MemoryStorage {
    data: Arc<Mutex<Data>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(Data::new())),
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl WebauthnStorage for MemoryStorage {
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
        passkey: Passkey,
    ) -> Result<(), StorageError> {
        let mut data = self.data.lock().await;

        // Check if email already exists
        if data.email_to_user_id.contains_key(&email) {
            return Err(StorageError::EmailAlreadyExists);
        }

        // Insert the new user with both identifiers
        data.email_to_user_id.insert(email, user_id.clone());
        data.user_id_to_webauthn_uuid
            .insert(user_id.clone(), webauthn_uuid);
        data.webauthn_uuid_to_user_id
            .insert(webauthn_uuid, user_id.clone());
        data.keys.insert(user_id, vec![passkey]);

        Ok(())
    }

    async fn get_user_passkeys(&self, user_id: &UserId) -> Result<Vec<Passkey>, StorageError> {
        let data = self.data.lock().await;
        Ok(data.keys.get(user_id).cloned().unwrap_or_default())
    }

    async fn add_passkey(&self, user_id: &UserId, passkey: Passkey) -> Result<(), StorageError> {
        let mut data = self.data.lock().await;

        match data.keys.get_mut(user_id) {
            Some(passkeys) => {
                passkeys.push(passkey);
                Ok(())
            }
            None => Err(StorageError::UserNotFound),
        }
    }

    async fn remove_passkey(
        &self,
        user_id: &UserId,
        passkey_id: &[u8],
    ) -> Result<bool, StorageError> {
        let mut data = self.data.lock().await;

        match data.keys.get_mut(user_id) {
            Some(passkeys) => {
                let initial_len = passkeys.len();
                passkeys.retain(|pk| pk.cred_id().as_slice() != passkey_id);
                Ok(passkeys.len() < initial_len)
            }
            None => Err(StorageError::UserNotFound),
        }
    }

    async fn get_all_credentials(&self) -> Result<Vec<Passkey>, StorageError> {
        let data = self.data.lock().await;
        let credentials = data.keys.values().flatten().cloned().collect();
        Ok(credentials)
    }

    async fn find_user_by_credential(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<UserId>, StorageError> {
        let data = self.data.lock().await;

        for (user_id, passkeys) in &data.keys {
            for passkey in passkeys {
                if passkey.cred_id().as_slice() == credential_id {
                    return Ok(Some(user_id.clone()));
                }
            }
        }

        Ok(None)
    }
}
