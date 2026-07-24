use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use std::fmt::Display;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Name(String);

impl Name {
    pub fn try_new(n: String) -> Result<Name, NameError> {
        if n.trim().is_empty() {
            Err(NameError::TooShort(n))
        } else if n.len() > 64 {
            Err(NameError::TooLong(n))
        } else {
            Ok(Name(n))
        }
    }
}

impl Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for Name {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Default for Name {
    fn default() -> Self {
        Self::try_new("examplename".to_string()).expect("valid default name")
    }
}

impl Type<Postgres> for Name {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&str as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for Name {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <&str as Encode<Postgres>>::encode(self.as_ref(), buf)
    }
}

impl<'r> Decode<'r, Postgres> for Name {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let str = <String as Decode<Postgres>>::decode(value)?;
        Ok(Name::try_new(str)?)
    }
}

#[derive(Error, Debug, Eq, PartialEq)]
pub enum NameError {
    #[error("The name {0} is too short")]
    TooShort(String),

    #[error("The name {0} is too long")]
    TooLong(String),
}
