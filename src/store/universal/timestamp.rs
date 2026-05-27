use axum_test::expect_json::__private::serde_trampoline::Deserialize;
use chrono::{DateTime, Utc};
use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::BigInt;
use diesel::{AsExpression, FromSqlRow, deserialize, serialize};

#[derive(Debug, Clone, PartialEq, Deserialize, Eq, AsExpression, FromSqlRow)]
#[diesel(sql_type = diesel::sql_types::BigInt)]
pub struct TimeStamp(pub DateTime<Utc>);

impl ToSql<BigInt, diesel::sqlite::Sqlite> for TimeStamp {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, diesel::sqlite::Sqlite>) -> serialize::Result {
        out.set_value(self.0.timestamp_millis());
        Ok(serialize::IsNull::No)
    }
}

impl ToSql<BigInt, diesel::pg::Pg> for TimeStamp {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, diesel::pg::Pg>) -> serialize::Result {
        let millis = self.0.timestamp_millis();
        <i64 as ToSql<BigInt, diesel::pg::Pg>>::to_sql(&millis, &mut out.reborrow())
    }
}

impl<DB: Backend> FromSql<BigInt, DB> for TimeStamp
where
    i64: FromSql<BigInt, DB>,
{
    fn from_sql(value: DB::RawValue<'_>) -> deserialize::Result<Self> {
        i64::from_sql(value).map(|val| {
            TimeStamp(DateTime::from_timestamp_millis(val).expect("failed to parse a timestamp"))
        })
    }
}
