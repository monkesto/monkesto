#![expect(dead_code)]

use super::grant::GrantStream;
use super::role::RoleStream;
use crate::authority::Authority;
use crate::grant::GrantId;
use crate::role::RoleId;
use crate::store::revised;

#[derive(Clone, Copy, Debug)]
pub struct AuthorizationStream;

impl revised::StreamFamily for AuthorizationStream {
    type Id = AuthorizationId;
    type Record = AuthorizationRecord;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthorizationId {
    Role(RoleId),
    Grant(GrantId),
}

#[derive(Clone)]
pub enum AuthorizationRecord {
    Role(revised::Record<RoleStream>),
    Grant(revised::Record<GrantStream>),
}

#[derive(Clone, Debug)]
pub enum AuthorizationEvent {
    Role(revised::Event<Authority, RoleStream>),
    Grant(revised::Event<Authority, GrantStream>),
}

impl revised::EventFamily for AuthorizationEvent {
    type Stream = AuthorizationStream;
    type Authority = Authority;

    fn event_id(&self) -> revised::EventId {
        match self {
            AuthorizationEvent::Role(event) => event.event_id,
            AuthorizationEvent::Grant(event) => event.event_id,
        }
    }

    fn id(&self) -> <Self::Stream as revised::StreamFamily>::Id {
        match self {
            AuthorizationEvent::Role(event) => AuthorizationId::Role(event.id),
            AuthorizationEvent::Grant(event) => AuthorizationId::Grant(event.id),
        }
    }
}
