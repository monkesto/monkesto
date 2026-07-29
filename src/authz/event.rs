use super::{GrantId, RoleId};
use crate::authority::{Actor, Authority};
use crate::name::Name;
use crate::time_provider::Timestamp;
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
        timestamp: Timestamp,
    },
    RoleActorAdded {
        #[id]
        role_id: RoleId,
        actor: Actor,
        authority: Authority,
        timestamp: Timestamp,
    },
    RoleActorRemoved {
        #[id]
        role_id: RoleId,
        actor: Actor,
        authority: Authority,
        timestamp: Timestamp,
    },
    GrantCreated {
        #[id]
        grant_id: GrantId,
        role_id: RoleId,
        authority: Authority,
        timestamp: Timestamp,
    },
    GrantRevoked {
        #[id]
        grant_id: GrantId,
        authority: Authority,
        timestamp: Timestamp,
    },
}
