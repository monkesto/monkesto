use super::event::{AggregateType, EventType};
use leptos::prelude::ServerFnError;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Hash)]
pub enum Permissions {
    Read,
    ReadWrite,
    Share,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum AccountEvent {
    Created {
        owner_id: Uuid,
        name: String,
        starting_balance: i64,
    },
    AddTenant {
        shared_user_id: Uuid,
        permissions: Permissions,
    },
    UpdateTenant {
        shared_user_id: Uuid,
        permissions: Permissions,
    },
    RemoveTenant {
        shared_user_id: Uuid,
    },
    BalanceUpdated {
        amount: i64,
    },
    Deleted,
}

impl AccountEvent {
    pub async fn push_db(&self, uuid: Uuid, pool: &PgPool) -> Result<i64, ServerFnError> {
        let payload = match serde_json::to_value(self) {
            Ok(s) => s,
            Err(e) => return Err(ServerFnError::ServerError(e.to_string())),
        };
        match sqlx::query_scalar(
            r#"
            INSERT INTO events (
                aggregate_id,
                aggregate_type,
                event_type,
                payload
            )
            VALUES ($1, $2, $3, $4)
            RETURNING id
            "#,
        )
        .bind(uuid)
        .bind(AggregateType::Account)
        .bind(EventType::from_account_event(self))
        .bind(payload)
        .fetch_one(pool)
        .await
        {
            Ok(s) => Ok(s),
            Err(e) => Err(ServerFnError::ServerError(e.to_string())),
        }
    }
}

#[derive(Default)]
struct AccountState {
    id: Uuid,
    balance: i64,
    tenants: std::collections::HashMap<Uuid, Permissions>,
    name: String,
    owner_id: Uuid,
    deleted: bool,
}

impl AccountState {
    pub fn from_events(id: Uuid, events: Vec<AccountEvent>) -> Self {
        let mut aggregate = Self {
            id,
            ..Default::default()
        };
        for event in events {
            aggregate.apply(event);
        }
        aggregate
    }

    pub fn apply(&mut self, event: AccountEvent) {
        match event {
            AccountEvent::Created {
                owner_id,
                name,
                starting_balance,
            } => {
                self.owner_id = owner_id;
                self.name = name;
                self.balance = starting_balance;
            }

            AccountEvent::BalanceUpdated { amount } => self.balance += amount,

            AccountEvent::AddTenant {
                shared_user_id,
                permissions,
            } => _ = self.tenants.insert(shared_user_id, permissions),

            AccountEvent::RemoveTenant { shared_user_id } => {
                _ = self.tenants.remove(&shared_user_id)
            }

            AccountEvent::UpdateTenant {
                shared_user_id,
                permissions,
            } => {
                _ = self
                    .tenants
                    .entry(shared_user_id)
                    .and_modify(|e| *e = permissions)
            }

            AccountEvent::Deleted => self.deleted = true,
        }
    }
}
