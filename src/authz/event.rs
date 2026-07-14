use super::{GrantId, RoleId};
use crate::authority::{Actor, Authority};
use crate::name::Name;
use chrono::{DateTime, Utc};
use disintegrate::Event;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Event, Serialize, Deserialize)]
#[stream(RoleEvent, [RoleCreated, RoleActorAdded, RoleActorRemoved])]
#[stream(GrantEvent, [GrantCreated, GrantRevoked])]
pub enum AuthzEvent {
    RoleCreated {
        #[id]
        role_id: RoleId,
        name: Name,
        authority: Authority,
        timestamp: DateTime<Utc>,
    },
    RoleActorAdded {
        #[id]
        role_id: RoleId,
        actor: Actor,
        authority: Authority,
        timestamp: DateTime<Utc>,
    },
    RoleActorRemoved {
        #[id]
        role_id: RoleId,
        actor: Actor,
        authority: Authority,
        timestamp: DateTime<Utc>,
    },
    GrantCreated {
        #[id]
        grant_id: GrantId,
        role_id: RoleId,
        authority: Authority,
        timestamp: DateTime<Utc>,
    },
    GrantRevoked {
        #[id]
        grant_id: GrantId,
        authority: Authority,
        timestamp: DateTime<Utc>,
    },
}
