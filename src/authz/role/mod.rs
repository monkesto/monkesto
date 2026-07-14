mod create;
mod index;
mod membership;
mod page;

pub use index::RoleIndex;
pub use page::router;

use crate::authority::{Actor, Authority};
use crate::authz::event::AuthzEvent;
use crate::id;
use crate::id::Ident;
use crate::name::Name;
use chrono::{DateTime, Utc};
use disintegrate::{PersistedEvent, StateMutate, StateQuery};
use disintegrate_postgres::PgEventId;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use thiserror::Error;

use super::event::RoleEvent;

id!(RoleId, Ident::new16());

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoleState {
    Absent,
    Present { name: Name, actors: HashSet<Actor> },
}

#[derive(Clone, Debug, StateQuery, Serialize, Deserialize)]
#[state_query(RoleEvent)]
pub struct Role {
    #[id]
    role_id: RoleId,
    name: Option<Name>,
    actors: HashSet<Actor>,
}

impl Role {
    pub fn new(role_id: RoleId) -> Self {
        Self {
            role_id,
            name: None,
            actors: HashSet::new(),
        }
    }

    pub fn exists(&self) -> bool {
        self.name.is_some()
    }
}

impl StateMutate for Role {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            RoleEvent::RoleCreated { name, .. } => self.name = Some(name),
            RoleEvent::RoleActorAdded { actor, .. } => {
                self.actors.insert(actor);
            }
            RoleEvent::RoleActorRemoved { actor, .. } => {
                self.actors.remove(&actor);
            }
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum RoleDecisionError {
    #[error("role already exists: {0}")]
    Exists(RoleId),
    #[error("role not found: {0}")]
    NotFound(RoleId),
}

pub struct CreateRole {
    role_id: RoleId,
    name: Name,
    authority: Authority,
    timestamp: DateTime<Utc>,
}

impl CreateRole {
    pub fn new(
        role_id: RoleId,
        name: Name,
        authority: Authority,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            role_id,
            name,
            authority,
            timestamp,
        }
    }
}

pub struct ChangeRoleActor {
    role_id: RoleId,
    actor: Actor,
    add: bool,
    authority: Authority,
    timestamp: DateTime<Utc>,
}

impl ChangeRoleActor {
    pub fn new(
        role_id: RoleId,
        actor: Actor,
        add: bool,
        authority: Authority,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            role_id,
            actor,
            add,
            authority,
            timestamp,
        }
    }
}

#[derive(Debug, Error)]
pub enum RoleIndexError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Postcard(#[from] postcard::Error),
    #[error(transparent)]
    Ident(#[from] crate::id::IdentError),
}

impl RoleIndex {
    pub async fn role(&self, role_id: RoleId) -> Result<RoleState, RoleIndexError> {
        self.find_role(role_id).await
    }

    pub async fn roles(&self, actor: &Actor) -> Result<HashSet<RoleId>, RoleIndexError> {
        self.find_roles(actor).await
    }

    pub async fn all_roles(&self) -> Result<Vec<(RoleId, RoleState)>, RoleIndexError> {
        self.find_all_roles().await
    }

    pub async fn project(
        &self,
        events: Vec<PersistedEvent<PgEventId, AuthzEvent>>,
    ) -> Result<(), RoleIndexError> {
        self.apply_all(events).await
    }
}
