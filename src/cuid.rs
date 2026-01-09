use super::known_errors::KnownErrors;
use cuid::{Cuid2Constructor, cuid2_slug, is_cuid2};
use serde::{Deserialize, Serialize};
use sqlx::{Decode, Encode, Type, postgres::PgValueRef};
use std::{fmt, str::FromStr};

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

impl FromStr for Cuid {
    type Err = KnownErrors;
    fn from_str(s: &str) -> Result<Self, KnownErrors> {
        if !is_cuid2(s) {
            return Err(KnownErrors::InvalidId);
        }
        match s.len() {
            10 => Ok(Self::Cuid10(s.as_bytes().try_into()?)),
            16 => Ok(Self::Cuid16(s.as_bytes().try_into()?)),
            _ => Err(KnownErrors::InvalidId),
        }
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

impl Type<sqlx::Postgres> for Cuid {
    fn type_info() -> <sqlx::Postgres as sqlx::Database>::TypeInfo {
        <&[u8] as Type<sqlx::Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, sqlx::Postgres> for Cuid {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Postgres as sqlx::Database>::ArgumentBuffer<'q>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        let bytes = self.as_bytes();
        <&[u8] as Encode<sqlx::Postgres>>::encode(bytes, buf)
    }
}

impl<'r> Decode<'r, sqlx::Postgres> for Cuid {
    fn decode(value: PgValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let bytes = <&[u8] as Decode<sqlx::Postgres>>::decode(value)?;
        Ok(Self::from_bytes(bytes)?)
    }
}

#[cfg(test)]
mod test_cuid {
    use sqlx::{PgPool, prelude::FromRow};

    use super::Cuid;

    #[sqlx::test]
    async fn test_encode_decode_cuid(pool: PgPool) {
        let original_id = Cuid::new10();

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

        #[derive(FromRow)]
        struct WrapperType {
            id: Cuid,
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
