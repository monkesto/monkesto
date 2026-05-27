use axum_test::expect_json::__private::serde_trampoline::{Deserialize, Serialize};
use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::BigInt;
use diesel::{AsExpression, FromSqlRow, deserialize, serialize};
use std::ops::{Add, Deref};

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    Eq,
    PartialOrd,
    Ord,
    Copy,
    AsExpression,
    FromSqlRow,
)]
#[diesel(sql_type = diesel::sql_types::BigInt)]
pub struct EventId(pub i64);

impl Add<i32> for EventId {
    type Output = EventId;

    fn add(self, rhs: i32) -> Self::Output {
        EventId(self.0 + rhs as i64)
    }
}

impl<DB: Backend> ToSql<BigInt, DB> for EventId
where
    i64: ToSql<BigInt, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, DB>) -> serialize::Result {
        self.0.to_sql(out)
    }
}

impl<DB: Backend> FromSql<BigInt, DB> for EventId
where
    i64: FromSql<BigInt, DB>,
{
    fn from_sql(value: DB::RawValue<'_>) -> deserialize::Result<Self> {
        i64::from_sql(value).map(EventId)
    }
}

impl Deref for EventId {
    type Target = i64;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
