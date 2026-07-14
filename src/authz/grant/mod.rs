mod create;
mod revoke;

use crate::authority::Authority;
use crate::id;
use crate::id::Ident;
use chrono::{DateTime, Utc};
use disintegrate::{StateMutate, StateQuery};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::RoleId;
use super::event::GrantEvent;

id!(GrantId, Ident::new16());

#[derive(Clone, Debug, StateQuery, Serialize, Deserialize)]
#[state_query(GrantEvent)]
pub struct Grant {
    #[id]
    grant_id: GrantId,
    found: bool,
    revoked: bool,
}

impl Grant {
    fn new(grant_id: GrantId) -> Self {
        Self {
            grant_id,
            found: false,
            revoked: false,
        }
    }
}

impl StateMutate for Grant {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            GrantEvent::GrantCreated { .. } => self.found = true,
            GrantEvent::GrantRevoked { .. } => self.revoked = true,
        }
    }
}

#[derive(Debug, Error, PartialEq)]
pub enum GrantDecisionError {
    #[error("role not found: {0}")]
    RoleNotFound(RoleId),
    #[error("grant already exists: {0}")]
    Exists(GrantId),
    #[error("grant not found: {0}")]
    NotFound(GrantId),
}

pub struct CreateGrant {
    grant_id: GrantId,
    role_id: RoleId,
    authority: Authority,
    timestamp: DateTime<Utc>,
}

impl CreateGrant {
    pub fn new(
        grant_id: GrantId,
        role_id: RoleId,
        authority: Authority,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            grant_id,
            role_id,
            authority,
            timestamp,
        }
    }
}

pub struct RevokeGrant {
    grant_id: GrantId,
    authority: Authority,
    timestamp: DateTime<Utc>,
}

impl RevokeGrant {
    pub fn new(grant_id: GrantId, authority: Authority, timestamp: DateTime<Utc>) -> Self {
        Self {
            grant_id,
            authority,
            timestamp,
        }
    }
}
