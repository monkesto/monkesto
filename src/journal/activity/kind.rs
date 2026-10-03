use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use thiserror::Error;

#[derive(Debug, Default, Copy, Clone, PartialEq, Serialize, Deserialize, Eq)]
#[repr(i8)]
pub enum ActivityKind {
    #[default]
    Income,
    Expense,
}

#[derive(Debug, Error, PartialEq)]
#[error("{0}")]
pub struct ActivityKindFromIntError(pub i8);

impl TryFrom<i8> for ActivityKind {
    type Error = ActivityKindFromIntError;

    fn try_from(value: i8) -> Result<Self, Self::Error> {
        match value {
            x if x == ActivityKind::Income as i8 => Ok(ActivityKind::Income),
            x if x == ActivityKind::Expense as i8 => Ok(ActivityKind::Expense),
            _ => Err(ActivityKindFromIntError(value)),
        }
    }
}

impl Type<Postgres> for ActivityKind {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&i16 as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for ActivityKind {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <i16 as Encode<Postgres>>::encode(*self as i16, buf)
    }
}

impl<'r> Decode<'r, Postgres> for ActivityKind {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let int16 = <i16 as Decode<Postgres>>::decode(value)?;
        Ok(Self::try_from(int16 as i8)?)
    }
}
