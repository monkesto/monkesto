use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Hash)]
pub(crate) enum Permissions {
    Read,
    ReadWrite,
    Share,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub(crate) enum AccountEvent {
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
