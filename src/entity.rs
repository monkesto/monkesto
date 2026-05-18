#[macro_export]
macro_rules! payload {
    (
        $any_payload_ctor:path,
        $(#[$meta:meta])*
        $vis:vis enum $enum_name:ident {
            $($variants:tt)*
        }
    ) => {
        #[derive(Payload, Clone, serde::Serialize, serde::Deserialize, Debug, diesel::AsExpression, diesel::FromSqlRow)]
        #[diesel(sql_type = diesel::sql_types::Binary)]
        $(#[$meta])*
        $vis enum $enum_name {
            $($variants)*
        }

        impl From<$enum_name> for $crate::store::universal::registry::AnyPayload {
            fn from(val: $enum_name) -> Self {
                $any_payload_ctor(val)
            }
        }

         #[allow(unused_imports)]
        use std::io::Write as _;

        impl diesel::serialize::ToSql<diesel::sql_types::Binary, diesel::sqlite::Sqlite>
            for $enum_name
        {
            fn to_sql<'b>(
                &'b self,
                out: &mut diesel::serialize::Output<'b, '_, diesel::sqlite::Sqlite>,
            ) -> diesel::serialize::Result {
                out.set_value(postcard::to_allocvec(self)?);
                Ok(diesel::serialize::IsNull::No)
            }
        }

        impl diesel::serialize::ToSql<diesel::sql_types::Binary, diesel::pg::Pg> for $enum_name {
            fn to_sql<'b>(
                &'b self,
                out: &mut diesel::serialize::Output<'b, '_, diesel::pg::Pg>,
            ) -> diesel::serialize::Result {
                out.write_all(&postcard::to_allocvec(self)?)?;
                Ok(diesel::serialize::IsNull::No)
            }
        }

        impl<DB: diesel::backend::Backend>
            diesel::deserialize::FromSql<diesel::sql_types::Binary, DB> for $enum_name
        where
            Vec<u8>: diesel::deserialize::FromSql<diesel::sql_types::Binary, DB>,
        {
            fn from_sql(value: DB::RawValue<'_>) -> diesel::deserialize::Result<Self> {
                let bytes = <Vec<u8> as diesel::deserialize::FromSql<
                    diesel::sql_types::Binary,
                    DB,
                >>::from_sql(value)?;
                Ok(postcard::from_bytes(&bytes)?)
            }
        }
    };
}

#[macro_export]
macro_rules! state {
    (
        $(#[$meta:meta])*
        $vis:vis struct $struct_name:ident {
            $($fields:tt)*
        }
    ) => {
        #[derive(serde::Serialize, serde::Deserialize, Clone, Debug, State, diesel::Queryable, diesel::QueryableByName, diesel::Selectable, diesel::Insertable, diesel::AsChangeset)]
        $(#[$meta])*
        $vis struct $struct_name {
            $($fields)*
        }
    };
}
