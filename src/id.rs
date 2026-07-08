use cuid::Cuid2Constructor;
use cuid::cuid2_slug;
use cuid::is_cuid2;
use disintegrate::{IdentifierType, IdentifierValue, IntoIdentifierValue};
use phf::phf_set;
use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use std::fmt::Display;
use std::fmt::{self};
use std::str::FromStr;
use thiserror::Error;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ident {
    Cuid10([u8; 10]),
    Cuid16([u8; 16]),
    Custom([u8; 5]),
}

#[derive(Debug, Error, Serialize, Deserialize, Clone, Eq, PartialEq)]
pub enum IdentError {
    #[error("Failed to parse the provided bytes: {0}")]
    Parse(String),

    #[error("The provided string is not a valid Ident: {0}")]
    InvalidId(String),
}

impl Ident {
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

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Ident::Cuid10(id) => id.as_ref(),
            Ident::Cuid16(id) => id.as_ref(),
            Ident::Custom(id) => id.as_ref(),
        }
    }
}

impl TryFrom<&[u8]> for Ident {
    type Error = IdentError;

    fn try_from(bytes: &[u8]) -> Result<Self, IdentError> {
        let str = str::from_utf8(bytes).map_err(|e| IdentError::Parse(e.to_string()))?;
        Self::from_str(str)
    }
}

// all of these ids must be exactly 5 ascii characters
static VALID_CUSTOM_IDENTS: phf::Set<&'static str> = phf_set! {
    "dylan",
    "grace",
    "henry",
    "annie",
    "isaac",
    "sarah",
};

impl FromStr for Ident {
    type Err = IdentError;
    fn from_str(s: &str) -> Result<Self, IdentError> {
        if !is_cuid2(s) {
            return Err(IdentError::InvalidId(s.to_owned()));
        }
        match s.len() {
            // try_into should only throw an error if the slice is larger than the expected size
            //
            // because we check the size in the switch statement, that error should not be possible
            5 => {
                if VALID_CUSTOM_IDENTS.contains(s) {
                    Ok(Self::Custom(
                        s.as_bytes().try_into().expect("custom Ident invalid size"),
                    ))
                } else {
                    Err(IdentError::InvalidId(s.to_owned()))
                }
            }
            10 => Ok(Self::Cuid10(
                s.as_bytes()
                    .try_into()
                    .expect("10-length Ident invalid size"),
            )),
            16 => Ok(Self::Cuid16(
                s.as_bytes()
                    .try_into()
                    .expect("16 length Ident invalid size"),
            )),
            _ => Err(IdentError::InvalidId(s.to_owned())),
        }
    }
}

// this has the potential to panic if the id is created manually rather than with helper functions
impl Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Ident::Cuid10(id) => write!(
                f,
                "{}",
                str::from_utf8(id).expect("failed to convert Cuid10 to string")
            ),
            Ident::Cuid16(id) => write!(
                f,
                "{}",
                str::from_utf8(id).expect("failed to convert Cuid16 to string")
            ),
            Ident::Custom(id) => write!(
                f,
                "{}",
                str::from_utf8(id).expect("failed to convert custom Ident to string")
            ),
        }
    }
}

impl Type<Postgres> for Ident {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <Vec<u8> as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for Ident {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes = self.as_bytes().to_vec();
        <Vec<u8> as Encode<Postgres>>::encode(bytes, buf)
    }
}

impl<'r> Decode<'r, Postgres> for Ident {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes = <Vec<u8> as Decode<Postgres>>::decode(value)?;
        Ok(Self::try_from(bytes.as_slice())?)
    }
}

impl IntoIdentifierValue for Ident {
    const TYPE: IdentifierType = IdentifierType::String;

    fn into_identifier_value(self) -> IdentifierValue {
        String::into_identifier_value(self.to_string())
    }
}

#[macro_export]
macro_rules! id {
    ($id_name:ident, $new_fn:expr) => {
        #[derive(
            ::serde::Serialize, ::serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash,
        )]
        pub struct $id_name($crate::id::Ident);

        impl $id_name {
            pub fn new() -> Self {
                Self($new_fn)
            }
        }

        impl ::std::ops::Deref for $id_name {
            type Target = $crate::id::Ident;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl ::std::str::FromStr for $id_name {
            type Err = $crate::id::IdentError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self($crate::id::Ident::from_str(s)?))
            }
        }

        impl ::std::fmt::Display for $id_name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl TryFrom<&[u8]> for $id_name {
            type Error = $crate::id::IdentError;

            fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
                Ok(Self($crate::id::Ident::try_from(bytes)?))
            }
        }

        impl ::sqlx::Type<::sqlx::Postgres> for $id_name {
            fn type_info() -> <::sqlx::Postgres as ::sqlx::Database>::TypeInfo {
                <Ident as ::sqlx::Type<::sqlx::Postgres>>::type_info()
            }
        }

        impl<'q> ::sqlx::Encode<'q, ::sqlx::Postgres> for $id_name {
            fn encode_by_ref(
                &self,
                buf: &mut <::sqlx::Postgres as ::sqlx::Database>::ArgumentBuffer<'q>,
            ) -> Result<::sqlx::encode::IsNull, ::sqlx::error::BoxDynError> {
                <$crate::id::Ident as ::sqlx::Encode<::sqlx::Postgres>>::encode(self.0, buf)
            }
        }

        impl<'r> ::sqlx::Decode<'r, ::sqlx::Postgres> for $id_name {
            fn decode(
                value: <::sqlx::Postgres as ::sqlx::Database>::ValueRef<'r>,
            ) -> Result<Self, ::sqlx::error::BoxDynError> {
                Ok(Self(<$crate::id::Ident as ::sqlx::Decode<
                    ::sqlx::Postgres,
                >>::decode(value)?))
            }
        }

        impl ::disintegrate::IntoIdentifierValue for $id_name {
            const TYPE: ::disintegrate::IdentifierType = ::disintegrate::IdentifierType::String;

            fn into_identifier_value(self) -> ::disintegrate::IdentifierValue {
                <String as ::disintegrate::IntoIdentifierValue>::into_identifier_value(
                    self.to_string(),
                )
            }
        }
    };
}
