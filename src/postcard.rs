use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use std::fmt::Debug;
use std::ops::{Deref, DerefMut};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

impl<T> Type<Postgres> for Postcard<T> {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&[u8] as Type<Postgres>>::type_info()
    }
}

impl<'q, T: Serialize> Encode<'q, Postgres> for Postcard<T> {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes = postcard::to_allocvec(self)?;
        <Vec<u8> as Encode<Postgres>>::encode(bytes, buf)
    }
}

impl<'r, T: DeserializeOwned> Decode<'r, Postgres> for Postcard<T> {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes = <&[u8] as Decode<Postgres>>::decode(value)?;
        Ok(postcard::from_bytes(bytes)?)
    }
}
