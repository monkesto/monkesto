use super::known_errors::KnownErrors;
use cuid::{Cuid2Constructor, cuid2_slug, is_cuid2};
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

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, KnownErrors> {
        let str = str::from_utf8(bytes)?;
        Self::from_str(str)
    }

    #[allow(clippy::should_implement_trait)]
    pub fn from_str(str: &str) -> Result<Self, KnownErrors> {
        if !is_cuid2(str) {
            return Err(KnownErrors::InvalidId);
        }
        match str.len() {
            10 => Ok(Self::Cuid10(str.as_bytes().try_into()?)),
            16 => Ok(Self::Cuid16(str.as_bytes().try_into()?)),
            _ => Err(KnownErrors::InvalidId),
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Cuid::Cuid10(id) => id.as_ref(),
            Cuid::Cuid16(id) => id.as_ref(),
        }
    }

    #[allow(dead_code)]
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
