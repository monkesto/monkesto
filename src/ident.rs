use crate::account::{AccountPayload, AccountProjection};
use crate::journal::{JournalPayload, JournalProjection};
use crate::store::universal::{EntityType, Payload, Projection};
use crate::transaction::{TransactionPayload, TransactionProjection};
use cuid::Cuid2Constructor;
use cuid::cuid2_slug;
use cuid::is_cuid2;
use phf::phf_set;
use serde::Deserialize;
use serde::Serialize;
use std::fmt::Display;
use std::fmt::{self};
use std::ops::Deref;
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

#[derive(Debug, Error, Clone, Deserialize)]
pub enum ProjectionFromPayloadError {
    #[error("Expected a \"Created\" enum variant, but found {0}")]
    IncorrectVariant(String),
}

pub trait EntityId<'a>:
    Deref<Target = Ident> + FromStr<Err = IdentError> + Display + TryFrom<&'a [u8]> + Clone + Copy
{
    type Payload: Payload<'a>;
    type Projection: Projection<'a, Self>;
    fn as_bytes(&self) -> &[u8];

    fn entity_type(&self) -> EntityType;
}

/// A macro to create an entity with the id of id_name and an associated payload, projection, and entity type
///
/// # Parameters
/// `id_name`: The name of the id to create along with the entity (`UserId`, for example)
///
/// `payload`: The payload type associated with this entity
///
/// `projection`: The projection type associated with this entity
///
/// `entity_type`: A variant of the `EntityType` enum this entity should be associated with
///
/// `new_fn`: A function that returns an `Ident`
///
/// # Examples
///
/// ```
/// entity!(UserId, UserPayload, UserProjection, EntityType::User, Ident::new16());
/// ```
#[macro_export]
macro_rules! entity {
    ($id_name: ident, $payload: ty, $projection: ty, $entity_type: expr, $new_fn: expr) => {
        #[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $id_name($crate::ident::Ident);

        impl $id_name {
            pub fn new() -> Self {
                Self($new_fn)
            }
        }

        impl Deref for $id_name {
            type Target = Ident;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl FromStr for $id_name {
            type Err = $crate::ident::IdentError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Ident::from_str(s)?))
            }
        }

        impl Display for $id_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl TryFrom<&[u8]> for $id_name {
            type Error = $crate::ident::IdentError;

            fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
                Ok(Self(Ident::try_from(bytes)?))
            }
        }

        impl $crate::ident::EntityId<'_> for $id_name {
            type Payload = $payload;
            type Projection = $projection;
            fn as_bytes(&self) -> &[u8] {
                self.deref().as_bytes()
            }
            fn entity_type(&self) -> EntityType {
                $entity_type
            }
        }
    };
}

entity!(
    JournalId,
    JournalPayload,
    JournalProjection,
    EntityType::Journal,
    Ident::new10()
);

entity!(
    AccountId,
    AccountPayload,
    AccountProjection,
    EntityType::Account,
    Ident::new10()
);

entity!(
    TransactionId,
    TransactionPayload,
    TransactionProjection,
    EntityType::Transaction,
    Ident::new16()
);
