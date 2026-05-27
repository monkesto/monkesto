use crate::account::AccountPayload;
use crate::auth::passkey::PasskeyPayload;
use crate::auth::user::UserPayload;
use crate::journal::JournalPayload;
use crate::payload_from_bytes_match;
use crate::store::universal::example_entity::ExamplePayload;
use crate::transaction::TransactionPayload;
use diesel::{AsExpression, FromSqlRow};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::Deserialize;

mod misc;

#[repr(i16)]
#[derive(
    Debug,
    Clone,
    PartialEq,
    Deserialize,
    Copy,
    Eq,
    AsExpression,
    FromSqlRow,
    TryFromPrimitive,
    IntoPrimitive,
)]
#[diesel(sql_type = diesel::sql_types::SmallInt)]
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
#[derive(Clone)]
pub enum AnyPayload {
    Example(ExamplePayload),
    Account(AccountPayload),
    Passkey(PasskeyPayload),
    User(UserPayload),
    Journal(JournalPayload),
    Transaction(TransactionPayload),
}

pub fn payload_from_bytes(bytes: &[u8], entity_type: EntityType) -> postcard::Result<AnyPayload> {
    payload_from_bytes_match! (
        bytes,
        entity_type,
        EntityType::Example => ExamplePayload,
        EntityType::Journal => JournalPayload,
        EntityType::Account => AccountPayload,
        EntityType::Transaction => TransactionPayload,
        EntityType::Passkey => PasskeyPayload,
        EntityType::User => UserPayload,
        // EntityType::Grant => GrantPayload,
        // EntityType::Role => RolePayload,
    )
    // NOTE: Grant and Role entity types do not have an associated payload, they will panic
}
