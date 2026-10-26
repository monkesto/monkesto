use crate::id;
use crate::id::Ident;
use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use thiserror::Error;

id!(DonationEntryId, Ident::new16());

#[repr(i16)]
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum DonationInstrument {
    Cash = 1,
    Check = 2,
    Online = 3,
}

#[derive(Debug, Error, PartialEq)]
#[error("invalid donation instrument: {}", 0)]
pub struct DonationInstrumentFromIntError(i16);

impl TryFrom<i16> for DonationInstrument {
    type Error = DonationInstrumentFromIntError;
    fn try_from(value: i16) -> Result<Self, Self::Error> {
        match value {
            value if value == DonationInstrument::Cash as i16 => Ok(DonationInstrument::Cash),
            value if value == DonationInstrument::Check as i16 => Ok(DonationInstrument::Check),
            value if value == DonationInstrument::Online as i16 => Ok(DonationInstrument::Online),
            _ => Err(DonationInstrumentFromIntError(value)),
        }
    }
}

impl Type<Postgres> for DonationInstrument {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <i16 as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for DonationInstrument {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <i16 as Encode<Postgres>>::encode(*self as i16, buf)
    }
}

impl<'r> Decode<'r, Postgres> for DonationInstrument {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        Ok(DonationInstrument::try_from(
            <i16 as Decode<Postgres>>::decode(value)?,
        )?)
    }
}
