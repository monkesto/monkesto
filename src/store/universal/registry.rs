use crate::account::AccountPayload;
use crate::auth::passkey::PasskeyPayload;
use crate::auth::user::UserPayload;
use crate::grant::GrantPayload;
use crate::journal::JournalPayload;
use crate::role::RolePayload;
use crate::store::universal::example_entity::ExamplePayload;
use crate::transaction::TransactionPayload;
use serde::Deserialize;

#[repr(i8)]
#[derive(Debug, Clone, PartialEq, Deserialize, sqlx::Type, Copy)]
pub enum EntityType {
    Journal = 0,
    Account = 1,
    Transaction = 2,
    Passkey = 3,
    User = 4,
    Grant = 5,
    Role = 6,
    Example = 7,
}

#[allow(clippy::large_enum_variant)]
pub enum AnyPayload {
    Account(AccountPayload),
    Passkey(PasskeyPayload),
    User(UserPayload),
    Journal(JournalPayload),
    Transaction(TransactionPayload),
    Grant(GrantPayload),
    Role(RolePayload),
    Example(ExamplePayload),
}
