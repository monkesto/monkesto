use crate::account::AccountPayload;
use crate::auth::user::UserPayload;
use crate::authority::Authority;
use crate::ident::EntityId;
use crate::store::universal::{
    AnyPayload, ApplyPayload, EntityType, Event, EventId, Payload, PayloadWithId, Projection,
    SequenceId, Store, StoreError, StoreResult,
};
use crate::transaction::{BalanceUpdate, EntryType, TransactionPayload};
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::{SqlitePool, SqliteTransaction, query, query_scalar};
use std::ops::Deref;
use std::str::FromStr;
use tower_sessions::SessionStore;

struct SimpleSqliteStore {
    pool: SqlitePool,
    session_store: tower_sessions_sqlx_store::SqliteStore,
}

impl SimpleSqliteStore {
    async fn new() -> Self {
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("Failed to create SQLite memory")
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await
            .expect("Failed to connect to SQLite database");
        sqlx::migrate!("./migrations/simple_sqlite")
            .run(&pool)
            .await
            .expect("Failed to migrate the simple sqlite database");
        let session_store = tower_sessions_sqlx_store::SqliteStore::new(pool.clone());

        Self {
            pool,
            session_store,
        }
    }

    async fn update_account_balances(
        &self,
        balance_updates: Vec<BalanceUpdate>,
        reverse: bool,
        tx: &mut SqliteTransaction<'_>,
    ) -> StoreResult<()> {
        for update in balance_updates {
            // ensure the account exists
            // TODO: check if the account is accessible from the journal doing the transaction
            let account_exists: bool = sqlx::query_scalar(
                r#"
                    SELECT EXISTS(SELECT 1 FROM entities WHERE entity_type = ? AND entity_id = ?)
                    "#,
            )
            .bind(EntityType::Account)
            .bind(update.account_id.as_bytes())
            .fetch_one(&mut **tx)
            .await?;

            if !account_exists {
                return Err(StoreError::AccountModifiedBeforeCreation(
                    *update.account_id.deref(),
                ));
            }

            let amount = if update.entry_type == EntryType::Credit {
                update.amount as i64
            } else {
                -(update.amount as i64)
            };

            sqlx::query(
                r#"
                    UPDATE account_balances SET balance = balance + ? WHERE account_id = ?
                    "#,
            )
            .bind(if reverse { -amount } else { amount })
            .bind(update.account_id.as_bytes())
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    // requiring a transaction id would be better here, but it is guaranteed at compile time that it will be one even if the compiler can't directly prove it
    async fn latest_transaction_payload<'a, T: EntityId<'a>>(
        &self,
        transaction_id: T,
        tx: &mut SqliteTransaction<'_>,
    ) -> StoreResult<TransactionPayload> {
        let raw_payloads: Vec<Vec<u8>> = sqlx::query_scalar(
            r#"
                SELECT payload FROM event_log WHERE entity_id = ? ORDER BY id DESC
                "#,
        )
        .bind(transaction_id.as_bytes())
        .fetch_all(&mut **tx)
        .await?;

        // TODO: skip variants that don't modify balances when they are introduced
        if let Some(last_raw_payload) = raw_payloads.last() {
            return TransactionPayload::from_bytes(last_raw_payload);
        }

        Err(StoreError::TransactionModifiedBeforeCreation(
            *transaction_id.deref(),
        ))
    }
}

impl Store for SimpleSqliteStore {
    async fn record<'a, I: EntityId<'a>>(
        &self,
        authority: Authority,
        at: DateTime<Utc>,
        entity_id: I,
        payload: I::Payload,
        expected_sequence: SequenceId,
    ) -> StoreResult<EventId> {
        let mut tx = self
            .pool
            .begin()
            .await
            .expect("Failed to begin transaction");

        let store_entity_type: Option<EntityType> = query_scalar(
            r#"
                SELECT entity_type FROM entities WHERE id = ?
                "#,
        )
        .bind(entity_id.as_bytes())
        .fetch_optional(&mut *tx)
        .await?;

        let id_entity_type = entity_id.entity_type();

        let projection = if payload.creates_entity() {
            // the entity should not already exist if it is being created
            if store_entity_type.is_some() {
                return Err(StoreError::EntityType {
                    expected: None,
                    found: store_entity_type,
                });
            }

            if expected_sequence != SequenceId(0) {
                return Err(StoreError::Sequence {
                    expected: SequenceId(0),
                    found: expected_sequence,
                });
            }

            query(
                r#"
                    INSERT INTO entities (id, entity_type) VALUES (?, ?);
                    "#,
            )
            .bind(entity_id.as_bytes())
            .bind(id_entity_type)
            .execute(&mut *tx)
            .await?;

            I::Projection::try_from(PayloadWithId {
                payload: payload.clone(),
                id: entity_id,
            })?
        } else {
            if store_entity_type != Some(id_entity_type) {
                return Err(StoreError::EntityType {
                    expected: Some(id_entity_type),
                    found: store_entity_type,
                });
            }

            let db_sequence_id: i64 = query_scalar(
                r#"
                    SELECT sequence_num FROM event_log WHERE entity_id = ?;
                    "#,
            )
            .bind(entity_id.as_bytes())
            .fetch_one(&mut *tx)
            .await?;

            let found_sequence_id = SequenceId(db_sequence_id as u64);

            if found_sequence_id != expected_sequence {
                return Err(StoreError::Sequence {
                    expected: expected_sequence,
                    found: found_sequence_id,
                });
            }

            let projection_bytes: Vec<u8> = query_scalar(
                r#"
                    SELECT projection FROM projections WHERE entity_id = ?
                    "#,
            )
            .bind(entity_id.as_bytes())
            .fetch_one(&mut *tx)
            .await?;

            let mut projection = I::Projection::from_bytes(&projection_bytes)?;

            projection.apply(&payload);

            projection
        };

        // apply the event
        let timestamp = at.timestamp_millis();

        let event_id: i64 = query_scalar(
            r#"
                INSERT INTO event_log (entity_id, payload, sequence_num, authority, timestamp) VALUES (?, ?, ?, ?, ?) RETURNING id
                "#
        ).bind(entity_id.as_bytes()).bind(payload.as_bytes()).bind((*expected_sequence + 1) as i64).bind(authority.as_bytes()).bind(timestamp).fetch_one(&mut *tx).await?;

        // update the projection
        sqlx::query(
            r#"
                UPDATE projections SET projection = ? WHERE entity_id = ?
                "#,
        )
        .bind(projection.as_bytes())
        .bind(entity_id.as_bytes())
        .execute(&mut *tx)
        .await?;

        // update lookup tables where necessary
        match payload.into() {
            // insert an entry into the balance table when creating an account
            AnyPayload::Account(AccountPayload::Created { journal_id, .. }) => {
                sqlx::query(
                    r#"
                        INSERT INTO account_lookup (account_id, journal_id) VALUES (?, ?)
                        "#,
                )
                .bind(entity_id.as_bytes())
                .bind(journal_id.as_bytes())
                .execute(&mut *tx)
                .await?;

                sqlx::query(
                    r#"
                        INSERT INTO account_balance (id, balance) VALUES (?, ?)
                        "#,
                )
                .bind(entity_id.as_bytes())
                .bind(0)
                .execute(&mut *tx)
                .await?;
            }

            // update account balances for a transaction
            AnyPayload::Transaction(transaction_payload) => {
                match transaction_payload {
                    TransactionPayload::Created { updates, .. } => {
                        self.update_account_balances(updates, false, &mut tx)
                            .await?;
                    }

                    TransactionPayload::UpdatedBalancedUpdates { new_balanceupdates } => {
                        // undo the latest prior updates
                        let old_updates =
                            match self.latest_transaction_payload(entity_id, &mut tx).await? {
                                TransactionPayload::Created { updates, .. } => updates,

                                TransactionPayload::UpdatedBalancedUpdates {
                                    new_balanceupdates,
                                } => new_balanceupdates,

                                TransactionPayload::Deleted => {
                                    return Err(StoreError::TransactionModifiedAfterDeletion(
                                        *entity_id.deref(),
                                    ));
                                }
                            };

                        self.update_account_balances(old_updates, true, &mut tx)
                            .await?;

                        // apply the new updates
                        self.update_account_balances(new_balanceupdates, false, &mut tx)
                            .await?;
                    }
                    TransactionPayload::Deleted => {
                        // just undo the old updates
                        let old_updates =
                            match self.latest_transaction_payload(entity_id, &mut tx).await? {
                                TransactionPayload::Created { updates, .. } => updates,

                                TransactionPayload::UpdatedBalancedUpdates {
                                    new_balanceupdates,
                                } => new_balanceupdates,

                                TransactionPayload::Deleted => {
                                    return Err(StoreError::TransactionModifiedAfterDeletion(
                                        *entity_id.deref(),
                                    ));
                                }
                            };

                        self.update_account_balances(old_updates, true, &mut tx)
                            .await?;
                    }
                }
            }

            // maintain an index of email -> user
            AnyPayload::User(user_payload) => match user_payload {
                UserPayload::Created {
                    email,
                    webauthn_uuid: _webauthn_uuid,
                } => {
                    sqlx::query(
                        r#"
                            INSERT INTO user_lookup (entity_id, email) VALUES (?, ?)
                            "#,
                    )
                    .bind(entity_id.as_bytes())
                    .bind(email.to_string())
                    .execute(&mut *tx)
                    .await?;
                }
                UserPayload::Deleted => {
                    sqlx::query(
                        r#"
                            DELETE FROM user_lookup WHERE entity_id = ?
                            "#,
                    )
                    .bind(entity_id.as_bytes())
                    .execute(&mut *tx)
                    .await?;
                }
            },

            _ => {}
        }

        tx.commit().await?;

        Ok(EventId(event_id as u64))
    }

    async fn replay_events<'a, I: EntityId<'a>>(
        &self,
        _entity_id: I,
        _starting_sequence: SequenceId,
    ) -> Vec<Event<'a, I>> {
        todo!()
    }

    async fn get_projection<'a, I: EntityId<'a>>(
        &self,
        _entity_id: I,
    ) -> StoreResult<(I::Projection, SequenceId)> {
        todo!()
    }

    async fn rebuild_projection<'a, I: EntityId<'a>>(
        &self,
        _entity_id: I,
        _events: Vec<Event<'a, I>>,
    ) -> StoreResult<()> {
        todo!()
    }

    async fn session_store(&self) -> &impl SessionStore {
        &self.session_store
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::query_scalar;

    #[tokio::test]
    async fn test_store_creation_and_migration() {
        let store = SimpleSqliteStore::new().await;

        let expected_tables = ["entities", "event_log", "user_lookup", "account_balance"];

        for table in expected_tables {
            let table_exists: bool = query_scalar(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?)",
            )
            .bind(table)
            .fetch_one(&store.pool)
            .await
            .unwrap();

            assert!(table_exists);
        }
    }
}
