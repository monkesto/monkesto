use super::{StorageError, UserStorage};
use sqlx::{PgPool, Row};
use std::sync::Arc;
use webauthn_rs::prelude::{Passkey, Uuid};

/// Database storage implementation using PostgreSQL
#[allow(dead_code)]
pub struct DatabaseStorage {
    pool: Arc<PgPool>,
}

#[allow(dead_code)]
impl DatabaseStorage {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool: Arc::new(pool),
        }
    }

    /// Initialize the database tables for user storage
    pub async fn migrate(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                id UUID PRIMARY KEY,
                email VARCHAR(255) UNIQUE NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS passkeys (
                id SERIAL PRIMARY KEY,
                user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                credential_id BYTEA UNIQUE NOT NULL,
                passkey_data BYTEA NOT NULL,
                created_at TIMESTAMPTZ DEFAULT NOW()
            );

            CREATE INDEX IF NOT EXISTS idx_passkeys_user_id ON passkeys(user_id);
            CREATE INDEX IF NOT EXISTS idx_passkeys_credential_id ON passkeys(credential_id);
            "#,
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl UserStorage for DatabaseStorage {
    async fn email_exists(&self, email: &str) -> Result<bool, StorageError> {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email = $1")
            .bind(email)
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(count > 0)
    }

    async fn get_user_email(&self, user_id: &Uuid) -> Result<String, StorageError> {
        let row = sqlx::query("SELECT email FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        match row {
            Some(row) => Ok(row.get("email")),
            None => Err(StorageError::UserNotFound),
        }
    }

    async fn create_user(
        &self,
        email: String,
        user_id: Uuid,
        passkey: Passkey,
    ) -> Result<(), StorageError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        // Insert user
        sqlx::query("INSERT INTO users (id, email) VALUES ($1, $2)")
            .bind(user_id)
            .bind(&email)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                if e.to_string().contains("unique") {
                    StorageError::EmailAlreadyExists
                } else {
                    StorageError::OperationFailed(e.to_string())
                }
            })?;

        // Serialize passkey
        let passkey_data = postcard::to_allocvec(&passkey)
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        // Insert passkey
        sqlx::query(
            "INSERT INTO passkeys (user_id, credential_id, passkey_data) VALUES ($1, $2, $3)",
        )
        .bind(user_id)
        .bind(passkey.cred_id().as_slice())
        .bind(&passkey_data)
        .execute(&mut *tx)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(())
    }

    async fn get_user_passkeys(&self, user_id: &Uuid) -> Result<Vec<Passkey>, StorageError> {
        let rows = sqlx::query("SELECT passkey_data FROM passkeys WHERE user_id = $1")
            .bind(user_id)
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        let mut passkeys = Vec::new();
        for row in rows {
            let passkey_data: Vec<u8> = row.get("passkey_data");
            let passkey: Passkey = postcard::from_bytes(&passkey_data)
                .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
            passkeys.push(passkey);
        }

        Ok(passkeys)
    }

    async fn add_passkey(&self, user_id: &Uuid, passkey: Passkey) -> Result<(), StorageError> {
        // Check if user exists
        let user_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_one(&*self.pool)
            .await
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        if user_exists == 0 {
            return Err(StorageError::UserNotFound);
        }

        // Serialize passkey
        let passkey_data = postcard::to_allocvec(&passkey)
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        // Insert passkey
        sqlx::query(
            "INSERT INTO passkeys (user_id, credential_id, passkey_data) VALUES ($1, $2, $3)",
        )
        .bind(user_id)
        .bind(passkey.cred_id().as_slice())
        .bind(&passkey_data)
        .execute(&*self.pool)
        .await
        .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(())
    }

    async fn remove_passkey(
        &self,
        user_id: &Uuid,
        passkey_id: &[u8],
    ) -> Result<bool, StorageError> {
        let result = sqlx::query("DELETE FROM passkeys WHERE user_id = $1 AND credential_id = $2")
            .bind(user_id)
            .bind(passkey_id)
            .execute(&*self.pool)
            .await
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        if result.rows_affected() == 0 {
            // Check if user exists at all
            let user_exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE id = $1")
                .bind(user_id)
                .fetch_one(&*self.pool)
                .await
                .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

            if user_exists == 0 {
                return Err(StorageError::UserNotFound);
            }

            Ok(false) // Passkey not found, but user exists
        } else {
            Ok(true) // Passkey was removed
        }
    }

    async fn get_all_credentials(&self) -> Result<Vec<Passkey>, StorageError> {
        let rows = sqlx::query("SELECT passkey_data FROM passkeys")
            .fetch_all(&*self.pool)
            .await
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        let mut passkeys = Vec::new();
        for row in rows {
            let passkey_data: Vec<u8> = row.get("passkey_data");
            let passkey: Passkey = postcard::from_bytes(&passkey_data)
                .map_err(|e| StorageError::OperationFailed(e.to_string()))?;
            passkeys.push(passkey);
        }

        Ok(passkeys)
    }

    async fn find_user_by_credential(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<Uuid>, StorageError> {
        let row = sqlx::query("SELECT user_id FROM passkeys WHERE credential_id = $1")
            .bind(credential_id)
            .fetch_optional(&*self.pool)
            .await
            .map_err(|e| StorageError::OperationFailed(e.to_string()))?;

        Ok(row.map(|r| r.get("user_id")))
    }
}
