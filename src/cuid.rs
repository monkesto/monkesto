use super::known_errors::KnownErrors;
use cuid::{Cuid2Constructor, cuid2_slug, is_cuid2};
use leptos::prelude::ServerFnError;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cuid {
    Cuid10([u8; 10]),
    Cuid16([u8; 16]),
}

impl Cuid {
    pub fn new10() -> Self {
        Self::Cuid10(
            cuid2_slug()
                .as_bytes()
                .try_into()
                .expect("failed to generate new cuid10"),
        )
    }
    pub fn new16() -> Self {
        Self::Cuid16(
            Cuid2Constructor::new()
                .with_length(16)
                .create_id()
                .as_bytes()
                .try_into()
                .expect("failed to generate new cuid16"),
        )
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ServerFnError> {
        let str = str::from_utf8(bytes)?;
        Self::from_str(str)
    }

    pub fn from_str(str: &str) -> Result<Self, ServerFnError> {
        if !is_cuid2(str) {
            return Err(ServerFnError::ServerError(
                KnownErrors::InvalidId.to_string()?,
            ));
        }
        match str.len() {
            10 => Ok(Self::Cuid10(str.as_bytes().try_into()?)),
            16 => Ok(Self::Cuid16(str.as_bytes().try_into()?)),
            _ => Err(ServerFnError::ServerError(
                KnownErrors::InvalidId.to_string()?,
            )),
        }
    }

    pub fn to_bytes(self) -> Vec<u8> {
        match self {
            Cuid::Cuid10(id) => Vec::from(id),
            Cuid::Cuid16(id) => Vec::from(id),
        }
    }
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

impl Default for Cuid {
    fn default() -> Self {
        Self::from_str("aaaaaaaaaa").expect("failed to generate default Cuid")
    }
}

// this has the potential to panic if the id is created manually rather than with helper functions
impl fmt::Display for Cuid {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Cuid::Cuid10(id) => write!(
                f,
                "{}",
                str::from_utf8(id).expect("failed to convert Cuid10 to string")
            ),
            Cuid::Cuid16(id) => write!(
                f,
                "{}",
                str::from_utf8(id).expect("failed to convert Cuid16 to string")
            ),
        }
    }
}
