use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::Text;
use diesel::{AsExpression, FromSqlRow, deserialize, serialize};
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Display;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, AsExpression, FromSqlRow)]
#[diesel(sql_type = Text)]
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

impl<DB: Backend> ToSql<Text, DB> for Name
where
    String: ToSql<Text, DB>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, DB>) -> serialize::Result {
        self.0.to_sql(out)
    }
}

impl<DB: Backend> FromSql<Text, DB> for Name
where
    String: FromSql<Text, DB>,
{
    fn from_sql(value: DB::RawValue<'_>) -> deserialize::Result<Self> {
        Ok(Name::try_new(String::from_sql(value)?)?)
    }
}

#[derive(Error, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum NameError {
    #[error("The name {0} is too short")]
    TooShort(String),

    #[error("The name {0} is too long")]
    TooLong(String),
}
