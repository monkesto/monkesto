use axum_test::expect_json::__private::serde_trampoline::{Deserialize, Serialize};
use disintegrate::{IdentifierType, IdentifierValue, IntoIdentifierValue};
use regex::Regex;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use std::fmt::Display;
use std::sync::LazyLock;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Email(String);

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EmailError {
    #[error("invalid email address")]
    RegexViolated(String),
}

static EMAIL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\w\-.]+@([\w-]+\.)+[\w-]{2,}$").expect("Regex parse failure"));

impl Email {
    pub fn try_new<T: Into<String>>(value: T) -> Result<Self, EmailError> {
        let sanitized = value.into().trim().to_lowercase();

        if EMAIL_REGEX.is_match(&sanitized) {
            return Ok(Self(sanitized));
        }

        Err(EmailError::RegexViolated(sanitized))
    }
}

impl Display for Email {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl AsRef<str> for Email {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Default for Email {
    fn default() -> Self {
        Email::try_new("default@example.com".to_string()).expect("valid default email")
    }
}

impl Type<Postgres> for Email {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&str as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for Email {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <&str as Encode<Postgres>>::encode(self.as_ref(), buf)
    }
}

impl<'r> Decode<'r, Postgres> for Email {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let str = <String as Decode<Postgres>>::decode(value)?;
        Ok(Email::try_new(str)?)
    }
}

impl IntoIdentifierValue for Email {
    const TYPE: IdentifierType = IdentifierType::String;

    fn into_identifier_value(self) -> IdentifierValue {
        String::into_identifier_value(self.0)
    }
}
