pub use crate::authz::GrantId;
use serde::Deserialize;
use serde::Serialize;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Copy)]
pub enum Actor {
    User(UserId),
    System,
    Anonymous,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Copy)]
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

use crate::authn::UserId;
use crate::error::DecodeError;
use crate::error::DecodeError::FieldRequired;
use crate::proto::authority::authority::proto_actor::ActorType;
use crate::proto::authority::authority::proto_authority::{AuthorityType, ProtoDelegatedAuthority};
use crate::proto::authority::authority::{ProtoActor, ProtoAuthority};
use std::str::FromStr;

impl From<Actor> for ProtoActor {
    fn from(value: Actor) -> Self {
        let actor = match value {
            Actor::User(id) => ActorType::User(id.to_string()),
            Actor::System => ActorType::System(()),
            Actor::Anonymous => ActorType::Anonymous(()),
        };

        ProtoActor {
            actor_type: Some(actor),
        }
    }
}

impl From<Authority> for ProtoAuthority {
    fn from(value: Authority) -> Self {
        let authority = match value {
            Authority::Direct(actor) => AuthorityType::Direct(actor.into()),
            Authority::Delegated {
                grantor,
                grant,
                grantee,
            } => AuthorityType::Delegated(ProtoDelegatedAuthority {
                grantor: Some(grantor.into()),
                grant_id: grant.to_string(),
                grantee: Some(grantee.into()),
            }),
        };

        ProtoAuthority {
            authority_type: Some(authority),
        }
    }
}

impl TryFrom<ProtoActor> for Actor {
    type Error = DecodeError;

    fn try_from(value: ProtoActor) -> Result<Self, Self::Error> {
        let actor = match value.actor_type.ok_or(FieldRequired)? {
            ActorType::User(id) => Actor::User(UserId::from_str(id.as_str())?),
            ActorType::System(_) => Actor::System,
            ActorType::Anonymous(_) => Actor::Anonymous,
        };

        Ok(actor)
    }
}

impl TryFrom<Option<ProtoActor>> for Actor {
    type Error = DecodeError;

    fn try_from(value: Option<ProtoActor>) -> Result<Self, Self::Error> {
        value.ok_or(FieldRequired)?.try_into()
    }
}

impl TryFrom<ProtoAuthority> for Authority {
    type Error = DecodeError;

    fn try_from(value: ProtoAuthority) -> Result<Self, Self::Error> {
        let authority = match value.authority_type.ok_or(FieldRequired)? {
            AuthorityType::Direct(actor) => Authority::Direct(actor.try_into()?),
            AuthorityType::Delegated(delegated) => Authority::Delegated {
                grantor: delegated.grantor.ok_or(FieldRequired)?.try_into()?,
                grant: GrantId::from_str(delegated.grant_id.as_str())?,
                grantee: delegated.grantee.ok_or(FieldRequired)?.try_into()?,
            },
        };

        Ok(authority)
    }
}

impl TryFrom<Option<ProtoAuthority>> for Authority {
    type Error = DecodeError;

    fn try_from(value: Option<ProtoAuthority>) -> Result<Self, Self::Error> {
        value.ok_or(FieldRequired)?.try_into()
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
