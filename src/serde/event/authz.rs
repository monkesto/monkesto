use crate::authz::event::AuthzEvent;
use crate::name::Name;
use crate::serde::error::ProtoError;
use crate::serde::error::ProtoError::FieldRequired;
use proto::event::authz::ProtoAuthzEvent;
use proto::event::authz::proto_authz_event::{
    AuthzEventType, ProtoGrantCreated, ProtoGrantRevoked, ProtoRoleActorAdded,
    ProtoRoleActorRemoved, ProtoRoleCreated,
};

impl From<AuthzEvent> for ProtoAuthzEvent {
    fn from(event: AuthzEvent) -> Self {
        let event = match event {
            AuthzEvent::RoleCreated {
                role_id,
                name,
                authority,
                timestamp,
            } => AuthzEventType::RoleCreated(ProtoRoleCreated {
                role_id: Some(role_id.into()),
                name: name.to_string(),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            AuthzEvent::RoleActorAdded {
                role_id,
                actor,
                authority,
                timestamp,
            } => AuthzEventType::RoleActorAdded(ProtoRoleActorAdded {
                role_id: Some(role_id.into()),
                actor: Some(actor.into()),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            AuthzEvent::RoleActorRemoved {
                role_id,
                actor,
                authority,
                timestamp,
            } => AuthzEventType::RoleActorRemoved(ProtoRoleActorRemoved {
                role_id: Some(role_id.into()),
                actor: Some(actor.into()),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            AuthzEvent::GrantCreated {
                grant_id,
                role_id,
                authority,
                timestamp,
            } => AuthzEventType::GrantCreated(ProtoGrantCreated {
                grant_id: Some(grant_id.into()),
                role_id: Some(role_id.into()),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
            AuthzEvent::GrantRevoked {
                grant_id,
                authority,
                timestamp,
            } => AuthzEventType::GrantRevoked(ProtoGrantRevoked {
                grant_id: Some(grant_id.into()),
                authority: Some(authority.into()),
                timestamp: Some(timestamp.into()),
            }),
        };
        ProtoAuthzEvent {
            authz_event_type: Some(event),
        }
    }
}

impl TryFrom<ProtoAuthzEvent> for AuthzEvent {
    type Error = ProtoError;

    fn try_from(event: ProtoAuthzEvent) -> Result<Self, Self::Error> {
        let event = match event.authz_event_type.ok_or(FieldRequired)? {
            AuthzEventType::RoleCreated(ev) => AuthzEvent::RoleCreated {
                role_id: ev.role_id.try_into()?,
                name: Name::try_new(ev.name)?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            AuthzEventType::RoleActorAdded(ev) => AuthzEvent::RoleActorAdded {
                role_id: ev.role_id.try_into()?,
                actor: ev.actor.try_into()?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            AuthzEventType::RoleActorRemoved(ev) => AuthzEvent::RoleActorRemoved {
                role_id: ev.role_id.try_into()?,
                actor: ev.actor.try_into()?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            AuthzEventType::GrantCreated(ev) => AuthzEvent::GrantCreated {
                grant_id: ev.grant_id.try_into()?,
                role_id: ev.role_id.try_into()?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
            AuthzEventType::GrantRevoked(ev) => AuthzEvent::GrantRevoked {
                grant_id: ev.grant_id.try_into()?,
                authority: ev.authority.try_into()?,
                timestamp: ev.timestamp.try_into()?,
            },
        };

        Ok(event)
    }
}
