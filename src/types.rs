#[cfg(feature = "ssr")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use uuid::Uuid;

use std::collections::{HashMap, HashSet};

#[derive(Serialize, Deserialize, Hash)]
enum Permissions {
    Read,
    Write,
}

#[cfg(feature = "ssr")]
struct AccountState {
    owner_id: Uuid,
    tenants: HashMap<Uuid, Permissions>,
    name: String,
    balance: i64,
    deleted: bool,
}

#[cfg(feature = "ssr")]
impl AccountState {
    fn apply(&mut self, event: AccountEvent) {
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

            AccountEvent::Deleted => self.deleted = true,
        }
    }
}

#[cfg(feature = "ssr")]
struct UserState {
    username: String,
    password: String,
    accounts: Vec<AccountState>,
    authenticated_sessions: HashSet<String>,
    deleted: bool,
}

#[cfg(feature = "ssr")]
impl UserState {
    fn apply(&mut self, event: UserEvent) {
        match event {
            UserEvent::Created { username, password } => {
                self.username = username;
                self.password = password;
            }
            UserEvent::UsernameUpdate { username } => self.username = username,
            UserEvent::PasswordUpdate { password } => self.password = password,
            UserEvent::Login { session_id } => _ = self.authenticated_sessions.insert(session_id),
            UserEvent::Logout { session_id } => _ = self.authenticated_sessions.remove(&session_id),
            UserEvent::Deleted => self.deleted = true,
        }
    }
}

#[cfg(feature = "ssr")]
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
enum UserEvent {
    Created { username: String, password: String },
    UsernameUpdate { username: String },
    PasswordUpdate { password: String },
    Login { session_id: String },
    Logout { session_id: String },
    Deleted,
}

#[cfg(feature = "ssr")]
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
enum AccountEvent {
    Created {
        owner_id: Uuid,
        name: String,
        starting_balance: i64,
    },
    BalanceUpdated {
        amount: i64,
    },
    AddTenant {
        shared_user_id: Uuid,
        permissions: Permissions,
    },
    RemoveTenant {
        shared_user_id: Uuid,
    },
    Deleted,
}
