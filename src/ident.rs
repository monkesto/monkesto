use crate::account::{AccountPayload, AccountProjection};
use crate::journal::{JournalPayload, JournalProjection};
use crate::store::universal::registry::EntityType;
use crate::transaction::{TransactionPayload, TransactionProjection};
use cuid::Cuid2Constructor;
use cuid::cuid2_slug;
use cuid::is_cuid2;
use phf::phf_set;
use serde::Deserialize;
use serde::Serialize;
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

#[derive(Debug, Error, Clone, Deserialize)]
pub enum ProjectionFromPayloadError {
    #[error("Expected a \"Created\" enum variant, but found {0}")]
    IncorrectVariant(String),
}

/// A macro to create an entity and associate it with an Id, Payload, and Projection type
///
/// # Constraints
/// The `Payload` type derives `Payload`, `Clone`, `Serialize`, `Deserialize`, and `Debug`
///
/// The `Projection` type derives `Clone`, `Serialize`, and `Deserialize`
///
/// The `Projection` type implements TryFrom<PayloadWithId<'_, {`entity_type`}Id>> with an
/// error type of `ProjectionFromPayloadError`. It should return `IncorrectVariant`
/// if the payload isn't the `Created` variant.
///
/// The `Projection` type implements `ApplyPayload<'_, {`entity_type`}Id>`.
/// It should leave the projection unchanged if the `Payload` is of the `Created` variant.
///
/// The `EntityType` and `AnyPayload` enums in crate::store::universal::registry are updated
/// to include the relevant variants for your entity.
///
///
/// # Parameters
/// `entity_type`: The name of the entity marker type to create. It should have the suffix `Entity`.
///
/// `registry_entry`: An entry in the universal store registry enum corresponding to the entity type.
///
/// `id_type`: The name of the id type to create. It should have the suffix `Id`.
///
/// `payload_type`: An existing payload type. It should have the suffix `Payload`.
///
/// `projection_type`: An existing projection type. It should have the suffix `Projection`.
///
/// `id_new_fn`: A function that returns an `Ident`
///
/// # Examples
///
/// ```
/// entity!(Example, Ident::new16());
/// ```
#[macro_export]
macro_rules! entity {
    ($entity_type: ident, $registry_entry: expr, $id_type: ident, $payload_type: ty, $projection_type: ty, $id_new_fn: expr) => {
        #[derive(serde::Serialize, serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $id_type($crate::ident::Ident);

        impl $id_type {
            pub fn new() -> Self {
                Self($id_new_fn)
            }
        }

        impl $crate::store::universal::EntityId for $id_type {
            fn as_bytes(&self) -> &[u8] {
                self.0.as_bytes()
            }
        }

        impl std::ops::Deref for $id_type {
            type Target = Ident;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl core::str::FromStr for $id_type {
            type Err = $crate::ident::IdentError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Ident::from_str(s)?))
            }
        }

        impl std::fmt::Display for $id_type {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl core::convert::TryFrom<&[u8]> for $id_type {
            type Error = $crate::ident::IdentError;

            fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
                Ok(Self(Ident::try_from(bytes)?))
            }
        }

        impl $crate::store::universal::Projection<'_, $entity_type> for $projection_type {}

        #[derive(Debug)]
        pub struct $entity_type;

        impl<'a> $crate::store::universal::Entity<'a> for $entity_type {
            type Id = $id_type;
            type Payload = $payload_type;
            type Projection = $projection_type;

            fn entity_type() -> $crate::store::universal::registry::EntityType {
                $registry_entry
            }
        }
    };
}

entity!(
    JournalEntity,
    EntityType::Journal,
    JournalId,
    JournalPayload,
    JournalProjection,
    Ident::new10()
);

entity!(
    AccountEntity,
    EntityType::Account,
    AccountId,
    AccountPayload,
    AccountProjection,
    Ident::new10()
);

entity!(
    TransactionEntity,
    EntityType::Transaction,
    TransactionId,
    TransactionPayload,
    TransactionProjection,
    Ident::new16()
);
