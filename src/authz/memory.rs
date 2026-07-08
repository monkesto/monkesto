#![cfg_attr(not(test), expect(dead_code))]

use super::RoleId;
use super::projection::AuthzProjection;
use super::role::RoleState;
use super::store::AuthzEvent;
use crate::authority::Actor;
use crate::store::memory::MemoryStore;
use crate::store::memory::SyncMemoryProjection;
use std::collections::HashMap;
use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::sync::Mutex;

pub type AuthzMemoryStore<P = AuthzMemoryProjection> = MemoryStore<AuthzEvent, P>;

#[derive(Clone, Default)]
pub struct AuthzMemoryProjection {
    state: Arc<Mutex<AuthzMemoryProjectionState>>,
}

#[derive(Default)]
struct AuthzMemoryProjectionState {
    roles: HashMap<RoleId, RoleState>,
}

impl AuthzProjection for AuthzMemoryProjection {
    type Error = Infallible;

    async fn role(&self, role_id: RoleId) -> Result<RoleState, Self::Error> {
        let state = self.state.lock().expect("poisoned");
        Ok(state
            .roles
            .get(&role_id)
            .cloned()
            .unwrap_or(RoleState::Absent))
    }

    async fn roles(&self, actor: &Actor) -> Result<HashSet<RoleId>, Self::Error> {
        let state = self.state.lock().expect("poisoned");
        Ok(state
            .roles
            .iter()
            .filter_map(|(role_id, role)| match role {
                RoleState::Present { actors, .. } if actors.contains(actor) => Some(*role_id),
                _ => None,
            })
            .collect())
    }

    async fn all_roles(&self) -> Result<Vec<(RoleId, RoleState)>, Self::Error> {
        let state = self.state.lock().expect("poisoned");
        Ok(state
            .roles
            .iter()
            .map(|(role_id, role)| (*role_id, role.clone()))
            .collect())
    }
}

impl SyncMemoryProjection<AuthzEvent> for AuthzMemoryProjection {
    type Error = Infallible;

    fn apply(&mut self, events: &[AuthzEvent]) -> Result<(), Self::Error> {
        let mut state = self.state.lock().expect("poisoned");
        for event in events {
            let AuthzEvent::Role(event) = event else {
                continue;
            };
            state
                .roles
                .entry(event.id)
                .or_insert(RoleState::Absent)
                .apply(event.clone());
        }

        Ok(())
    }
}
