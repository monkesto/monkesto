use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use std::ops::Deref;

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct CorePasskey(pub webauthn_rs::prelude::Passkey);

// todo: figure out why this wasn't implemented in the original type
impl Eq for CorePasskey {}

impl Deref for CorePasskey {
    type Target = webauthn_rs::prelude::Passkey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Type<Postgres> for CorePasskey {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&[u8] as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for CorePasskey {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes = rmp_serde::to_vec(self)?;
        <Vec<u8> as Encode<Postgres>>::encode(bytes, buf)
    }
}

impl<'r> Decode<'r, Postgres> for CorePasskey {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes = <&[u8] as Decode<Postgres>>::decode(value)?;
        Ok(rmp_serde::from_slice(bytes)?)
    }
}
