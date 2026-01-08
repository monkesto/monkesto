use super::{StorageError, UserStorage};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;
use webauthn_rs::prelude::{Passkey, Uuid};

/// Legacy data structure for in-memory storage
pub struct Data {
    pub email_to_id: HashMap<String, Uuid>,
    pub keys: HashMap<Uuid, Vec<Passkey>>,
}

impl Data {
    pub fn new() -> Self {
        Self {
            email_to_id: HashMap::new(),
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
impl UserStorage for MemoryStorage {
    async fn email_exists(&self, email: &str) -> Result<bool, StorageError> {
        let data = self.data.lock().await;
        Ok(data.email_to_id.contains_key(email))
    }

    async fn get_user_email(&self, user_id: &Uuid) -> Result<String, StorageError> {
        let data = self.data.lock().await;
        data.email_to_id
            .iter()
            .find_map(|(email, id)| {
                if *id == *user_id {
                    Some(email.clone())
                } else {
                    None
                }
            })
            .ok_or(StorageError::UserNotFound)
    }

    async fn create_user(
        &self,
        email: String,
        user_id: Uuid,
        passkey: Passkey,
    ) -> Result<(), StorageError> {
        let mut data = self.data.lock().await;

        // Check if email already exists
        if data.email_to_id.contains_key(&email) {
            return Err(StorageError::EmailAlreadyExists);
        }

        // Insert the new user
        data.email_to_id.insert(email, user_id);
        data.keys.insert(user_id, vec![passkey]);

        Ok(())
    }

    async fn get_user_passkeys(&self, user_id: &Uuid) -> Result<Vec<Passkey>, StorageError> {
        let data = self.data.lock().await;
        Ok(data.keys.get(user_id).cloned().unwrap_or_default())
    }

    async fn add_passkey(&self, user_id: &Uuid, passkey: Passkey) -> Result<(), StorageError> {
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
        user_id: &Uuid,
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

    async fn find_user_by_credential(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<Uuid>, StorageError> {
        let data = self.data.lock().await;

        for (user_id, passkeys) in &data.keys {
            for passkey in passkeys {
                if passkey.cred_id().as_slice() == credential_id {
                    return Ok(Some(*user_id));
                }
            }
        }

        Ok(None)
    }

    async fn has_any_users(&self) -> Result<bool, StorageError> {
        let data = self.data.lock().await;
        Ok(!data.email_to_id.is_empty())
    }
}
