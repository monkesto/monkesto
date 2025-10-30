#[cfg(feature = "ssr")]
pub(crate) mod types {
    use serde::{Deserialize, Serialize};

    use uuid::Uuid;

    use std::collections::{HashMap, HashSet};

    #[derive(Serialize, Deserialize, Hash)]
    enum Permissions {
        Read,
        ReadWrite,
        Share,
    }

    struct AccountState {
        balance: i64,
        tenants: HashMap<Uuid, Permissions>,
        name: String,
        owner_id: Uuid,
        deleted: bool,
    }

    impl AccountState {
        fn new(uuid: Uuid) -> Self {
            AccountState {
                balance: 0,
                tenants: HashMap::new(),
                name: "".to_string(),
                owner_id: uuid,
                deleted: false,
            }
        }

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

    struct UserState {
        authenticated_sessions: HashSet<String>,
        username: String,
        password: String,
        accounts: HashSet<Uuid>,
        deleted: bool,
    }

    impl UserState {
        fn new() -> Self {
            UserState {
                authenticated_sessions: HashSet::new(),
                username: "".to_string(),
                password: "".to_string(),
                accounts: HashSet::new(),
                deleted: false,
            }
        }

        fn apply(&mut self, event: UserEvent) {
            match event {
                UserEvent::Created { username, password } => {
                    self.username = username;
                    self.password = password;
                }
                UserEvent::AddAccount { id } => _ = self.accounts.insert(id),
                UserEvent::UsernameUpdate { username } => self.username = username,
                UserEvent::PasswordUpdate { password } => self.password = password,
                UserEvent::Login { session_id } => {
                    _ = self.authenticated_sessions.insert(session_id)
                }
                UserEvent::Logout { session_id } => {
                    _ = self.authenticated_sessions.remove(&session_id)
                }
                UserEvent::Deleted => self.deleted = true,
            }
        }
    }

    #[derive(Serialize, Deserialize)]
    #[serde(tag = "type", content = "data")]
    enum UserEvent {
        Created { username: String, password: String },
        UsernameUpdate { username: String },
        PasswordUpdate { password: String },
        Login { session_id: String },
        Logout { session_id: String },
        AddAccount { id: Uuid },
        Deleted,
    }

    #[derive(Serialize, Deserialize)]
    #[serde(tag = "type", content = "data")]
    enum AccountEvent {
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
}
