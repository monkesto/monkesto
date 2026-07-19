pub(crate) use crate::authn::user::UserId;
pub use crate::authz::GrantId;
use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Actor {
    User(UserId),
    System,
    Anonymous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Authority {
    Direct(Actor),
    Delegated {
        grantor: Actor,
        grant: GrantId,
        grantee: Actor,
    },
}

impl Authority {
    pub fn actor(&self) -> &Actor {
        match self {
            Authority::Direct(actor) => actor,
            Authority::Delegated { grantee, .. } => grantee,
        }
    }

    pub fn user_id(&self) -> Option<UserId> {
        match self.actor() {
            Actor::Anonymous => None,
            Actor::System => None,
            Actor::User(user_id) => Some(*user_id),
        }
    }
}

impl Type<Postgres> for Actor {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&[u8] as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for Actor {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        let bytes = postcard::to_allocvec(self)?;
        <Vec<u8> as Encode<Postgres>>::encode(bytes, buf)
    }
}

impl<'r> Decode<'r, Postgres> for Actor {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let bytes = <&[u8] as Decode<Postgres>>::decode(value)?;
        Ok(postcard::from_bytes(bytes)?)
    }
}

impl TryFrom<&[u8]> for Authority {
    type Error = postcard::Error;
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        postcard::from_bytes(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_userid_new() {
        let user_id = UserId::new();
        assert_eq!(user_id.to_string().len(), 16);
    }

    #[test]
    fn test_multiple_generated_ids_are_unique() {
        let id1 = UserId::new();
        let id2 = UserId::new();
        assert_ne!(id1, id2);
    }
}
