use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::Binary;
use diesel::{AsExpression, FromSqlRow, deserialize, serialize};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;
use std::io::Write;
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, AsExpression, FromSqlRow)]
#[diesel(sql_type = Binary)]
pub struct Postcard<T>(pub T);

impl<T> Deref for Postcard<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<T> DerefMut for Postcard<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: Serialize + Debug> ToSql<Binary, diesel::sqlite::Sqlite> for Postcard<T> {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, diesel::sqlite::Sqlite>) -> serialize::Result {
        let bytes = postcard::to_allocvec(&self.0)?;
        out.set_value(bytes);
        Ok(serialize::IsNull::No)
    }
}

impl<T: Serialize + Debug> ToSql<Binary, diesel::pg::Pg> for Postcard<T> {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, diesel::pg::Pg>) -> serialize::Result {
        let bytes = postcard::to_allocvec(&self.0)?;
        out.write_all(&bytes)?;
        Ok(serialize::IsNull::No)
    }
}

impl<T: DeserializeOwned, DB: Backend> FromSql<Binary, DB> for Postcard<T>
where
    Vec<u8>: FromSql<Binary, DB>,
{
    fn from_sql(value: DB::RawValue<'_>) -> deserialize::Result<Self> {
        let bytes = <Vec<u8> as FromSql<Binary, DB>>::from_sql(value)?;
        Ok(postcard::from_bytes(&bytes)?)
    }
}
