#[macro_export]
macro_rules! payload {
    (
        $any_payload_ctor:path,
        $(#[$meta:meta])*
        $vis:vis enum $enum_name:ident {
            $($variants:tt)*
        }
    ) => {
        #[derive(Payload, Clone, serde::Serialize, serde::Deserialize, Debug)]
        $(#[$meta])*
        $vis enum $enum_name {
            $($variants)*
        }

        impl From<$enum_name> for $crate::store::universal::registry::AnyPayload {
            fn from(val: $enum_name) -> Self {
                $any_payload_ctor(val)
            }
        }
    };
}

#[macro_export]
macro_rules! projection {
    (
        $(#[$meta:meta])*
        $vis:vis struct $struct_name:ident {
            $($fields:tt)*
        }
    ) => {
        #[derive(serde::Serialize, serde::Deserialize, Clone, Debug, Projection, sqlx::FromRow, sqlx::Decode)]
        $(#[$meta])*
        $vis struct $struct_name {
            $($fields)*
        }
    };
}
