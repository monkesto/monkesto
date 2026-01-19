use super::{Passkey, PasskeyStore, StorageError};
use crate::webauthn::passkey::PasskeyId;
use crate::webauthn::user::UserId;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

pub struct PasskeyData {
    pub keys: HashMap<UserId, Vec<Passkey>>,
}

impl PasskeyData {
    pub fn new() -> Self {
        Self {
            keys: HashMap::new(),
        }
    }
}

impl Default for PasskeyData {
    fn default() -> Self {
        Self::new()
    }
}

/// In-memory storage implementation for passkeys using HashMap
pub struct MemoryPasskeyStore {
    data: Arc<Mutex<PasskeyData>>,
}

impl MemoryPasskeyStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(PasskeyData::new())),
        }
    }
}

impl Default for MemoryPasskeyStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl PasskeyStore for MemoryPasskeyStore {
    type EventId = ();
    type Error = StorageError;

    async fn get_user_passkeys(&self, user_id: &UserId) -> Result<Vec<Passkey>, StorageError> {
        let data = self.data.lock().await;
        Ok(data.keys.get(user_id).cloned().unwrap_or_default())
    }

    async fn add_passkey(
        &self,
        user_id: &UserId,
        passkey_id: PasskeyId,
        passkey: webauthn_rs::prelude::Passkey,
    ) -> Result<(), StorageError> {
        let mut data = self.data.lock().await;

        // Create entry if user doesn't exist in passkey store yet
        let passkeys = data.keys.entry(*user_id).or_insert_with(Vec::new);
        passkeys.push(Passkey {
            id: passkey_id,
            passkey,
        });

        Ok(())
    }

    async fn remove_passkey(
        &self,
        user_id: &UserId,
        passkey_id: &PasskeyId,
    ) -> Result<bool, StorageError> {
        let mut data = self.data.lock().await;

        match data.keys.get_mut(user_id) {
            Some(passkeys) => {
                let initial_len = passkeys.len();
                passkeys.retain(|stored| &stored.id != passkey_id);
                Ok(passkeys.len() < initial_len)
            }
            None => Ok(false), // User has no passkeys, so nothing was removed
        }
    }

    async fn get_all_credentials(
        &self,
    ) -> Result<Vec<webauthn_rs::prelude::Passkey>, StorageError> {
        let data = self.data.lock().await;
        let credentials = data
            .keys
            .values()
            .flatten()
            .map(|stored| stored.passkey.clone())
            .collect();
        Ok(credentials)
    }

    async fn find_user_by_credential(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<(UserId, PasskeyId)>, StorageError> {
        let data = self.data.lock().await;

        for (user_id, passkeys) in &data.keys {
            for stored in passkeys {
                if stored.passkey.cred_id().as_slice() == credential_id {
                    return Ok(Some((*user_id, stored.id)));
                }
            }
        }

        Ok(None)
    }
}
