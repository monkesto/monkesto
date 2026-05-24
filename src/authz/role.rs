use crate::authority::Actor;
use crate::authority::Authority;
use crate::name::Name;
use crate::role::RoleId;
use crate::role::RolePayload;
use crate::store::revised::Event;
use crate::store::revised::EventId;
use crate::store::revised::When;
use std::collections::HashSet;

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
    pub fn apply(&mut self, event: Event<Authority, RoleId, RolePayload>) {
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
