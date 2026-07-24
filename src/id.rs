use arrayvec::ArrayString;
use cuid::Cuid2Constructor;
use cuid::cuid2_slug;
use cuid::is_cuid2;
use disintegrate::{IdentifierType, IdentifierValue, IntoIdentifierValue};
use phf::phf_set;
use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use std::fmt::Display;
use std::fmt::{self};
use std::str::FromStr;
use thiserror::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Ident {
    Cuid10(ArrayString<10>),
    Cuid16(ArrayString<16>),
    Custom(ArrayString<5>),
}

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum IdentError {
    #[error("Failed to parse the provided bytes: {0}")]
    Parse(String),

    #[error("The provided string is not a valid Ident: {0}")]
    InvalidId(String),
}

impl Ident {
    pub fn new10() -> Self {
        Self::Cuid10(
            ArrayString::from(cuid2_slug().as_str()).expect("generated cuid10 string too large"),
        )
    }
    pub fn new16() -> Self {
        Self::Cuid16(
            ArrayString::from(Cuid2Constructor::new().with_length(16).create_id().as_str())
                .expect("generated cuid16 string too large"),
        )
    }

    pub fn nil() -> Self {
        Self::from_str("uinit").expect("nil cuid guaranteed to be valid")
    }

    pub fn as_str(&self) -> &str {
        match self {
            Ident::Cuid10(id) => id.as_str(),
            Ident::Cuid16(id) => id.as_str(),
            Ident::Custom(id) => id.as_str(),
        }
    }
}

impl Default for Ident {
    fn default() -> Self {
        Self::nil()
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
    "uinit"
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
                        ArrayString::from(s).expect("custom ident too large"),
                    ))
                } else {
                    Err(IdentError::InvalidId(s.to_owned()))
                }
            }
            10 => Ok(Self::Cuid10(
                ArrayString::from(s).expect("10-length Ident invalid size"),
            )),
            16 => Ok(Self::Cuid16(
                ArrayString::from(s).expect("16-length Ident invalid size"),
            )),
            _ => Err(IdentError::InvalidId(s.to_owned())),
        }
    }
}

impl From<String> for Ident {
    fn from(value: String) -> Self {
        Self::from_str(value.as_str()).expect("invalid ident")
    }
}

impl Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Ident::Cuid10(id) => write!(f, "{id}",),
            Ident::Cuid16(id) => write!(f, "{id}",),
            Ident::Custom(id) => write!(f, "{id}",),
        }
    }
}

impl Type<Postgres> for Ident {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&str as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for Ident {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <&str as Encode<Postgres>>::encode(self.as_str(), buf)
    }
}

impl<'r> Decode<'r, Postgres> for Ident {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let str = <&str as Decode<Postgres>>::decode(value)?;
        Ok(Self::from_str(str)?)
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

            pub fn nil() -> Self {
                Self($crate::id::Ident::nil())
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

        impl From<String> for $id_name {
            fn from(value: String) -> Self {
                Self(<Ident as From<String>>::from(value))
            }
        }
        impl ::std::fmt::Display for $id_name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl ::sqlx::Type<::sqlx::Postgres> for $id_name {
            fn type_info() -> <::sqlx::Postgres as ::sqlx::Database>::TypeInfo {
                <&str as ::sqlx::Type<::sqlx::Postgres>>::type_info()
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

        impl Default for $id_name {
            fn default() -> Self {
                Self(Default::default())
            }
        }
    };
}
