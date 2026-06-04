use axum_test::expect_json::__private::serde_trampoline::{Deserialize, Serialize};
use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::Text;
use diesel::{AsExpression, FromSqlRow, deserialize, serialize};
use regex::Regex;
use std::fmt::Display;
use std::sync::LazyLock;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, AsExpression, FromSqlRow)]
#[diesel(sql_type = Text)]
pub struct Email(String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Eq, Error)]
pub enum EmailError {
    #[error("invalid email address")]
    RegexViolated,
}

static EMAIL_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[\w\-.]+@([\w-]+\.)+[\w-]{2,}$").expect("Regex parse failure"));

impl Email {
    pub fn try_new<T: Into<String>>(value: T) -> Result<Self, EmailError> {
        let sanitized = value.into().trim().to_lowercase();

        if EMAIL_REGEX.is_match(&sanitized) {
            return Ok(Self(sanitized));
        }

        Err(EmailError::RegexViolated)
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

impl<DB: Backend> ToSql<Text, DB> for Email
where
    String: ToSql<Text, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, DB>) -> serialize::Result {
        self.0.to_sql(out)
    }
}

impl<DB: Backend> FromSql<Text, DB> for Email
where
    String: FromSql<Text, DB>,
{
    fn from_sql(value: DB::RawValue<'_>) -> deserialize::Result<Self> {
        Ok(Email::try_new(String::from_sql(value)?)?)
    }
}
