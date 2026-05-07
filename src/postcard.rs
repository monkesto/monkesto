use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::sqlite::{SqliteArgumentValue, SqliteTypeInfo, SqliteValueRef};
use sqlx::{Decode, Encode, FromRow, Sqlite, Type};
use std::borrow::Cow;
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, FromRow)]
pub struct Postcard<T>(pub T);

impl<T> Type<Sqlite> for Postcard<T> {
    fn type_info() -> SqliteTypeInfo {
        <&[u8] as Type<Sqlite>>::type_info()
    }
}

impl<'q, T: Serialize> Encode<'q, Sqlite> for Postcard<T> {
    fn encode_by_ref(
        &self,
        args: &mut Vec<SqliteArgumentValue<'q>>,
    ) -> Result<IsNull, BoxDynError> {
        args.push(SqliteArgumentValue::Blob(Cow::Owned(
            postcard::to_allocvec(&self.0).expect("failed to serialize"),
        )));
        Ok(IsNull::No)
    }
}

impl<'r, T: Deserialize<'r>> Decode<'r, Sqlite> for Postcard<T> {
    fn decode(value: SqliteValueRef<'r>) -> Result<Self, BoxDynError> {
        let s = <&[u8] as Decode<Sqlite>>::decode(value)?;
        Ok(postcard::from_bytes(s)?)
    }
}

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
