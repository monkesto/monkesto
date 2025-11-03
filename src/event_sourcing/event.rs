use super::account::AccountEvent;
use super::user::UserEvent;

#[derive(sqlx::Type)]
#[sqlx(type_name = "smallint")]
#[repr(i16)]
enum AggregateType {
    User = 1,
    Account = 2,
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "smallint")]
#[repr(i16)]
enum EventType {
    // User events (1-99)
    UserCreated = 1,
    UsernameUpdate = 2,
    UserPasswordUpdate = 3,
    UserLogin = 4,
    UserLogout = 5,
    UserAddAccount = 6,
    UserDeleted = 7,

    // Account events (100-199)
    AccountCreated = 100,
    AccountAddTenant = 101,
    AccountUpdateTenant = 102,
    AccountRemoveTenant = 103,
    AccountBalanceUpdated = 104,
    AccountDeleted = 105,
}

impl EventType {
    fn from_user_event(user_event: UserEvent) -> Self {
        match user_event {
            UserEvent::Created { .. } => Self::UserCreated,
            UserEvent::UsernameUpdate { .. } => Self::UsernameUpdate,
            UserEvent::PasswordUpdate { .. } => Self::UserPasswordUpdate,
            UserEvent::Login { .. } => Self::UserLogin,
            UserEvent::Logout { .. } => Self::UserLogout,
            UserEvent::AddAccount { .. } => Self::UserAddAccount,
            UserEvent::Deleted => Self::UserDeleted,
        }
    }

    fn from_account_event(account_event: AccountEvent) -> Self {
        match account_event {
            AccountEvent::Created { .. } => Self::AccountCreated,
            AccountEvent::AddTenant { .. } => Self::AccountAddTenant,
            AccountEvent::UpdateTenant { .. } => Self::AccountUpdateTenant,
            AccountEvent::RemoveTenant { .. } => Self::AccountRemoveTenant,
            AccountEvent::BalanceUpdated { .. } => Self::AccountBalanceUpdated,
            AccountEvent::Deleted => Self::AccountDeleted,
        }
    }
}
