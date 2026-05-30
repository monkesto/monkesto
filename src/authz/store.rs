use super::grant::GrantStream;
use super::role::RoleStream;
use crate::authority::Authority;
use crate::grant::GrantId;
use crate::role::RoleId;
use crate::store::revised::Event;
use crate::store::revised::EventFamily;
use crate::store::revised::EventId;
use crate::store::revised::Record;
use crate::store::revised::RecordFor;
use crate::store::revised::Store;
use crate::store::revised::When;
use chrono::DateTime;
use chrono::Utc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AuthzId {
    Role(RoleId),
    Grant(GrantId),
}

#[derive(Clone)]
pub enum AuthzRecord {
    Role(Record<RoleStream>),
    Grant(Record<GrantStream>),
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
    type Record = AuthzRecord;
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

impl RecordFor<AuthzEvent> for AuthzRecord {
    fn id(&self) -> AuthzId {
        match self {
            AuthzRecord::Role(record) => AuthzId::Role(record.id),
            AuthzRecord::Grant(record) => AuthzId::Grant(record.id),
        }
    }

    fn when(&self) -> When<EventId> {
        match self {
            AuthzRecord::Role(record) => record.when,
            AuthzRecord::Grant(record) => record.when,
        }
    }

    fn into_event(
        self,
        event_id: EventId,
        authority: Authority,
        timestamp: DateTime<Utc>,
    ) -> AuthzEvent {
        match self {
            AuthzRecord::Role(record) => AuthzEvent::Role(Event {
                event_id,
                timestamp,
                authority,
                id: record.id,
                payload: record.payload,
            }),
            AuthzRecord::Grant(record) => AuthzEvent::Grant(Event {
                event_id,
                timestamp,
                authority,
                id: record.id,
                payload: record.payload,
            }),
        }
    }
}
