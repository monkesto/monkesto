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
    Example = 1,
    Journal = 2,
    Account = 3,
    Transaction = 4,
    Passkey = 5,
    User = 6,
    Grant = 7,
    Role = 8,
}

#[allow(clippy::large_enum_variant)]
pub enum AnyPayload {
    Example(ExamplePayload),
    Account(AccountPayload),
    Passkey(PasskeyPayload),
    User(UserPayload),
    Journal(JournalPayload),
    Transaction(TransactionPayload),
    Grant(GrantPayload),
    Role(RolePayload),
}
