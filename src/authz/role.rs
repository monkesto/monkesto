use crate::authority::Actor;
use crate::authority::Authority;
use crate::id;
use crate::ident::Ident;
use crate::name::Name;
use crate::store::Event;
use crate::store::EventId;
use crate::store::Stream;
use crate::store::When;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashSet;

id!(RoleId, Ident::new16());

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum RolePayload {
    Created(Name),
    ActorAdded(Actor),
    ActorRemoved(Actor),
}

#[derive(Clone, Copy, Debug)]
pub struct RoleStream;

impl Stream for RoleStream {
    type Id = RoleId;
    type Payload = RolePayload;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoleState {
    Absent,
    Present {
        name: Name,
        actors: HashSet<Actor>,
        when: When<EventId>,
    },
}

impl RoleState {
    pub fn apply(&mut self, event: Event<Authority, RoleStream>) {
        let when = When::Within(event.event_id);
        *self = match (self.clone(), event.payload) {
            (RoleState::Absent, RolePayload::Created(name)) => RoleState::Present {
                name,
                actors: HashSet::new(),
                when,
            },
            (
                RoleState::Present {
                    name, mut actors, ..
                },
                RolePayload::ActorAdded(actor),
            ) => {
                actors.insert(actor);
                RoleState::Present { name, actors, when }
            }
            (
                RoleState::Present {
                    name, mut actors, ..
                },
                RolePayload::ActorRemoved(actor),
            ) => {
                actors.remove(&actor);
                RoleState::Present { name, actors, when }
            }
            (state, _) => state,
        };
    }
}
