use serde::Deserialize;
use serde::Serialize;
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

#[derive(Error, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum NameError {
    #[error("The name {0} is too short")]
    TooShort(String),

    #[error("The name {0} is too long")]
    TooLong(String),
}
