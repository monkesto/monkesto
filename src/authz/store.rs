#![expect(dead_code)]

use crate::authority::Authority;
use crate::grant::GrantId;
use crate::grant::GrantPayload;
use crate::role::RoleId;
use crate::role::RolePayload;
use crate::store::revised::Event;
use crate::store::revised::EventFamily;
use crate::store::revised::EventId;
use crate::store::revised::Record;
use crate::store::revised::RecordFor;
use crate::store::revised::When;
use crate::store::revised::memory::MemoryStore;
use chrono::DateTime;
use chrono::Utc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AuthzId {
    Role(RoleId),
    Grant(GrantId),
}

#[derive(Clone)]
pub enum AuthzRecord {
    Role(Record<RoleId, RolePayload>),
    Grant(Record<GrantId, GrantPayload>),
}

#[derive(Clone, Debug)]
pub enum AuthzEvent {
    Role(Event<Authority, RoleId, RolePayload>),
    Grant(Event<Authority, GrantId, GrantPayload>),
}

pub type AuthzMemoryStore = MemoryStore<AuthzEvent>;
pub type Authz2MemoryStore = MemoryStore<AuthzEvent>;

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
