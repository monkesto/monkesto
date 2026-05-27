use crate::store::universal::registry::EntityType;
use diesel::backend::Backend;
use diesel::deserialize::FromSql;
use diesel::serialize::{Output, ToSql};
use diesel::sql_types::SmallInt;
use diesel::{deserialize, serialize};

impl ToSql<SmallInt, diesel::sqlite::Sqlite> for EntityType {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, diesel::sqlite::Sqlite>) -> serialize::Result {
        // SqliteBindValue doesn't implement From<i16>
        out.set_value(*self as i32);
        Ok(serialize::IsNull::No)
    }
}

impl ToSql<SmallInt, diesel::pg::Pg> for EntityType {
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, diesel::pg::Pg>) -> serialize::Result {
        <i16 as ToSql<SmallInt, diesel::pg::Pg>>::to_sql(&(*self as i16), &mut out.reborrow())
    }
}

impl<DB: Backend> FromSql<SmallInt, DB> for EntityType
where
    i16: FromSql<SmallInt, DB>,
{
    fn from_sql(value: DB::RawValue<'_>) -> deserialize::Result<Self> {
        Ok(EntityType::try_from(i16::from_sql(value)?)?)
    }
}

#[macro_export]
macro_rules! payload_from_bytes_match {
    ($bytes: ident, $entity_type: ident, $( $variant:path => $payload_type:ty),* $(,)?) => {
        match $entity_type{
                $(
                    $variant => Ok(postcard::from_bytes::<$payload_type>(
                        $bytes,
                    )?.into()),
                )*
                EntityType::Grant | EntityType::Role => todo!("grant and role entities do not have suitable payload types yet")
            }
    };
}
