use crate::account::AccountPayload;
use crate::auth::passkey::PasskeyPayload;
use crate::auth::user::UserPayload;
use crate::journal::JournalPayload;
use crate::store::universal::example_entity::ExamplePayload;
use crate::transaction::TransactionPayload;
use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::SmallInt;
use diesel::{AsExpression, FromSqlRow, deserialize, serialize};
use serde::Deserialize;
use thiserror::Error;

#[repr(i16)]
#[derive(Debug, Clone, PartialEq, Deserialize, Copy, Eq, AsExpression, FromSqlRow)]
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

#[derive(Error, Debug)]
pub enum EntityTypeFromPrimitiveError {
    #[error("The passed value is out of range for EntityType: {0}")]
    OutsideOfRange(i16),
}

impl TryFrom<i16> for EntityType {
    type Error = EntityTypeFromPrimitiveError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        use EntityType::*;

        match value {
            1 => Ok(Example),
            2 => Ok(Journal),
            3 => Ok(Account),
            4 => Ok(Transaction),
            5 => Ok(Passkey),
            6 => Ok(User),
            7 => Ok(Grant),
            8 => Ok(Role),
            _ => Err(EntityTypeFromPrimitiveError::OutsideOfRange(value)),
        }
    }
}

impl ToSql<SmallInt, diesel::sqlite::Sqlite> for EntityType {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, diesel::sqlite::Sqlite>) -> serialize::Result {
        // SqliteBindValue doesn't implement From<i16>
        out.set_value(*self as i32);
        Ok(serialize::IsNull::No)
    }
}

impl ToSql<SmallInt, diesel::pg::Pg> for EntityType {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, diesel::pg::Pg>) -> serialize::Result {
        <i16 as ToSql<SmallInt, diesel::pg::Pg>>::to_sql(&(*self as i16), &mut out.reborrow())
    }
}

impl<DB: Backend> FromSql<SmallInt, DB> for EntityType
where
    i16: FromSql<SmallInt, DB>,
{
    fn from_sql(value: DB::RawValue<'_>) -> deserialize::Result<Self> {
        Ok(EntityType::try_from(i16::from_sql(value)?)?)
    }
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
