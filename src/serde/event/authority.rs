use crate::authn::UserId;
use crate::authority::{Actor, Authority, GrantId};
use crate::proto::event::authority::proto_actor::ActorType;
use crate::proto::event::authority::proto_authority::{AuthorityType, ProtoDelegatedAuthority};
use crate::proto::event::authority::{ProtoActor, ProtoAuthority};
use crate::serde::error::ProtoError;
use crate::serde::error::ProtoError::FieldRequired;
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
    type Error = ProtoError;

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
    type Error = ProtoError;

    fn try_from(value: Option<ProtoActor>) -> Result<Self, Self::Error> {
        value.ok_or(FieldRequired)?.try_into()
    }
}

impl TryFrom<ProtoAuthority> for Authority {
    type Error = ProtoError;

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
    type Error = ProtoError;

    fn try_from(value: Option<ProtoAuthority>) -> Result<Self, Self::Error> {
        value.ok_or(FieldRequired)?.try_into()
    }
}
