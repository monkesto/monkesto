use super::GrantId;
use super::RoleId;
use super::grant::GrantPayload;
use super::grant::GrantStream;
use super::role::RolePayload;
use super::role::RoleStream;
use crate::authority::Authority;
use crate::store::Event;
use crate::store::EventFamily;
use crate::store::EventFor;
use crate::store::EventId;
use crate::store::Store;
use crate::store::sqlite::SqliteStreamId;
use chrono::DateTime;
use chrono::Utc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AuthzId {
    Role(RoleId),
    Grant(GrantId),
}

#[derive(Clone, Debug)]
pub enum AuthzEvent {
    Role(Event<Authority, RoleStream>),
    Grant(Event<Authority, GrantStream>),
}

pub trait AuthzStore: Store<AuthzEvent> {}

impl<S> AuthzStore for S where S: Store<AuthzEvent> {}

impl EventFamily for AuthzEvent {
    type Id = AuthzId;
    type Authority = Authority;

    fn event_id(&self) -> EventId {
        match self {
            AuthzEvent::Role(event) => event.event_id,
            AuthzEvent::Grant(event) => event.event_id,
        }
    }

    fn id(&self) -> Self::Id {
        match self {
            AuthzEvent::Role(event) => AuthzId::Role(event.id),
            AuthzEvent::Grant(event) => AuthzId::Grant(event.id),
        }
    }
}

impl SqliteStreamId for AuthzId {
    fn stream_type(&self) -> i64 {
        match self {
            AuthzId::Role(_) => 1,
            AuthzId::Grant(_) => 2,
        }
    }

    fn stream_id(&self) -> Vec<u8> {
        match self {
            AuthzId::Role(id) => id.as_bytes().to_vec(),
            AuthzId::Grant(id) => id.as_bytes().to_vec(),
        }
    }
}

impl EventFor<RoleStream> for AuthzEvent {
    fn id_for(id: RoleId) -> AuthzId {
        AuthzId::Role(id)
    }

    fn new_event(
        event_id: EventId,
        authority: Authority,
        timestamp: DateTime<Utc>,
        id: RoleId,
        payload: RolePayload,
    ) -> AuthzEvent {
        AuthzEvent::Role(Event {
            event_id,
            timestamp,
            authority,
            id,
            payload,
        })
    }
}

impl EventFor<GrantStream> for AuthzEvent {
    fn id_for(id: GrantId) -> AuthzId {
        AuthzId::Grant(id)
    }

    fn new_event(
        event_id: EventId,
        authority: Authority,
        timestamp: DateTime<Utc>,
        id: GrantId,
        payload: GrantPayload,
    ) -> AuthzEvent {
        AuthzEvent::Grant(Event {
            event_id,
            timestamp,
            authority,
            id,
            payload,
        })
    }
}
