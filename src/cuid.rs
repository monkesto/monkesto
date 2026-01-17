use super::known_errors::KnownErrors;
use cuid::{Cuid2Constructor, cuid2_slug, is_cuid2};
use phf::phf_set;
use serde::{Deserialize, Serialize};
use sqlx::{Decode, Encode, Type, postgres::PgValueRef};
use std::{
    fmt::{self, Display},
    ops::Deref,
    str::FromStr,
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ident {
    Cuid10([u8; 10]),
    Cuid16([u8; 16]),
    Custom([u8; 5]),
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

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, KnownErrors> {
        let str = str::from_utf8(bytes)?;
        Self::from_str(str)
    }

    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Ident::Cuid10(id) => id.as_ref(),
            Ident::Cuid16(id) => id.as_ref(),
            Ident::Custom(id) => id.as_ref(),
        }
    }
}

// all of these ids must be exactly 5 ascii characters
static VALID_CUSTOM_CUIDS: phf::Set<&'static str> = phf_set! {
    "dylan",
};

impl FromStr for Ident {
    type Err = KnownErrors;
    fn from_str(s: &str) -> Result<Self, KnownErrors> {
        if !is_cuid2(s) {
            return Err(KnownErrors::InvalidId);
        }
        match s.len() {
            5 => {
                if VALID_CUSTOM_CUIDS.contains(s) {
                    Ok(Self::Custom(s.as_bytes().try_into()?))
                } else {
                    Err(KnownErrors::InvalidId)
                }
            }
            10 => Ok(Self::Cuid10(s.as_bytes().try_into()?)),
            16 => Ok(Self::Cuid16(s.as_bytes().try_into()?)),
            _ => Err(KnownErrors::InvalidId),
        }
    }
}

// this has the potential to panic if the id is created manually rather than with helper functions
impl fmt::Display for Ident {
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
                str::from_utf8(id).expect("failed to convert custom Cuid to string")
            ),
        }
    }
}

macro_rules! id {
    ($name: ident, $new_fn: expr) => {
        #[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub struct $name(Ident);

        #[allow(dead_code)]
        impl $name {
            pub fn new() -> Self {
                Self($new_fn)
            }

            pub fn from_bytes(bytes: &[u8]) -> Result<Self, KnownErrors> {
                Ok(Self(Ident::from_bytes(bytes)?))
            }
        }

        impl Deref for $name {
            type Target = Ident;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl FromStr for $name {
            type Err = KnownErrors;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(Ident::from_str(s)?))
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

id!(UserId, Ident::new10());

id!(JournalId, Ident::new10());

id!(AccountId, Ident::new10());

id!(TransactionId, Ident::new16());

impl Type<sqlx::Postgres> for Ident {
    fn type_info() -> <sqlx::Postgres as sqlx::Database>::TypeInfo {
        <&[u8] as Type<sqlx::Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, sqlx::Postgres> for Ident {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Postgres as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let bytes = self.as_bytes();
        <&[u8] as Encode<sqlx::Postgres>>::encode(bytes, buf)
    }
}

impl<'r> Decode<'r, sqlx::Postgres> for Ident {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let bytes = <&[u8] as Decode<sqlx::Postgres>>::decode(value)?;
        Ok(Self::from_bytes(bytes)?)
    }
}

#[cfg(test)]
mod test_cuid {
    use sqlx::{PgPool, prelude::FromRow};

    use super::Ident;

    #[sqlx::test]
    async fn test_encode_decode_cuid(pool: PgPool) {
        let original_id = Ident::new10();

        sqlx::query(
            r#"
            CREATE TABLE test_cuid_table (
            id BYTEA
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("failed to create mock cuid table");

        sqlx::query(
            r#"
            INSERT INTO test_cuid_table(
            id
            )
            VALUES ($1)
            "#,
        )
        .bind(original_id)
        .execute(&pool)
        .await
        .expect("failed to insert cuid into mock table");

        let id: Ident = sqlx::query_scalar(
            r#"
            SELECT id FROM test_cuid_table
            LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed to fetch cuid from mock table");

        assert_eq!(id, original_id);

        #[derive(FromRow)]
        struct WrapperType {
            id: Ident,
        }

        let id_wrapper: WrapperType = sqlx::query_as(
            r#"
            SELECT id FROM test_cuid_table
            LIMIT 1
            "#,
        )
        .fetch_one(&pool)
        .await
        .expect("failed to fetch cuid from mock table");

        assert_eq!(id_wrapper.id, original_id)
    }
}
