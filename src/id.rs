#[macro_export]
macro_rules! id {
    ($id_name:ident, $new_fn:expr) => {
        #[derive(
            ::serde::Serialize, ::serde::Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash,
        )]
        pub struct $id_name($crate::ident::Ident);

        impl $id_name {
            pub fn new() -> Self {
                Self($new_fn)
            }
        }

        impl ::std::ops::Deref for $id_name {
            type Target = $crate::ident::Ident;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl ::std::str::FromStr for $id_name {
            type Err = $crate::ident::IdentError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self($crate::ident::Ident::from_str(s)?))
            }
        }

        impl ::std::fmt::Display for $id_name {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl TryFrom<&[u8]> for $id_name {
            type Error = $crate::ident::IdentError;

            fn try_from(bytes: &[u8]) -> Result<Self, Self::Error> {
                Ok(Self($crate::ident::Ident::try_from(bytes)?))
            }
        }
    };
}
