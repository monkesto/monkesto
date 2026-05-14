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
macro_rules! state {
    (
        $(#[$meta:meta])*
        $vis:vis struct $struct_name:ident {
            $($fields:tt)*
        }
    ) => {
        #[derive(serde::Serialize, serde::Deserialize, Clone, Debug, State, diesel::Queryable, diesel::QueryableByName ,diesel::Selectable, diesel::Insertable)]
        $(#[$meta])*
        $vis struct $struct_name {
            $($fields)*
        }
    };
}
