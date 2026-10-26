use crate::id;
use crate::id::Ident;
use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use std::fmt::{Display, Formatter};
use thiserror::Error;

pub mod donation_entry;
pub mod donor;

id!(DonationId, Ident::new16());

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckNum(String);

#[derive(Debug, Error, PartialEq)]
#[error("Check numbers must be a maximum of 50 characters in length")]
pub struct CheckNumFromIntError(String);

impl CheckNum {
    // for now, allow any kind of character in the check-num field to match jewel
    pub fn try_new(s: String) -> Result<CheckNum, CheckNumFromIntError> {
        if s.len() > 50 {
            return Err(CheckNumFromIntError(s));
        }

        Ok(CheckNum(s))
    }
}

impl Display for CheckNum {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Type<Postgres> for CheckNum {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&str as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for CheckNum {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <&str as Encode<Postgres>>::encode(self.to_string().as_str(), buf)
    }
}

impl<'r> Decode<'r, Postgres> for CheckNum {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let str = <String as Decode<Postgres>>::decode(value)?;
        Ok(CheckNum::try_new(str)?)
    }
}
