#![allow(dead_code)]

use super::GrantId;
use super::event::AuthzEvent;
use super::grant::{CreateGrant, GrantDecisionError, RevokeGrant};
use super::role::{
    ChangeRoleActor, CreateRole, RoleDecisionError, RoleId, RoleIndex, RoleIndexError, RoleState,
};
use super::store::AuthzEventStore;
use crate::authority::{Actor, Authority};
use crate::name::Name;
use chrono::Utc;
use disintegrate::{DecisionError, PersistedEvent};
use disintegrate_postgres::PgEventId;
use std::collections::HashSet;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthzError {
    #[error(transparent)]
    RoleDecision(#[from] RoleDecisionError),
    #[error(transparent)]
    GrantDecision(#[from] GrantDecisionError),
    #[error("disintegrate error: {0}")]
    Disintegrate(String),
    #[error(transparent)]
    RoleIndex(#[from] RoleIndexError),
    #[error("role lookup requires system authority")]
    RoleRequiresSystemAuthority,
}

#[derive(Clone)]
pub struct AuthzService {
    event_store: AuthzEventStore,
    role_index: RoleIndex,
}

impl AuthzService {
    pub fn new(event_store: AuthzEventStore, role_index: RoleIndex) -> Self {
        Self {
            event_store,
            role_index,
        }
    }

    pub async fn create_role(
        &self,
        authority: Authority,
        name: Name,
    ) -> Result<RoleId, AuthzError> {
        let role_id = RoleId::new();
        let events = self
            .event_store
            .decision_maker
            .make(CreateRole::new(role_id, name, authority, Utc::now()))
            .await
            .map_err(map_role_decision_error)?;
        self.project(events).await?;
        Ok(role_id)
    }

    pub async fn grant_role(
        &self,
        authority: Authority,
        role_id: RoleId,
    ) -> Result<GrantId, AuthzError> {
        let grant_id = GrantId::new();
        let events = self
            .event_store
            .decision_maker
            .make(CreateGrant::new(grant_id, role_id, authority, Utc::now()))
            .await
            .map_err(map_grant_decision_error)?;
        self.project(events).await?;
        Ok(grant_id)
    }

    pub async fn revoke_grant(
        &self,
        authority: Authority,
        grant_id: GrantId,
    ) -> Result<(), AuthzError> {
        let events = self
            .event_store
            .decision_maker
            .make(RevokeGrant::new(grant_id, authority, Utc::now()))
            .await
            .map_err(map_grant_decision_error)?;
        self.project(events).await
    }

    pub async fn add_role_actor(
        &self,
        authority: Authority,
        role_id: RoleId,
        actor: Actor,
    ) -> Result<(), AuthzError> {
        self.change_role_actor(authority, role_id, actor, true)
            .await
    }

    pub async fn remove_role_actor(
        &self,
        authority: Authority,
        role_id: RoleId,
        actor: Actor,
    ) -> Result<(), AuthzError> {
        self.change_role_actor(authority, role_id, actor, false)
            .await
    }

    async fn change_role_actor(
        &self,
        authority: Authority,
        role_id: RoleId,
        actor: Actor,
        add: bool,
    ) -> Result<(), AuthzError> {
        let events = self
            .event_store
            .decision_maker
            .make(ChangeRoleActor::new(
                role_id,
                actor,
                add,
                authority,
                Utc::now(),
            ))
            .await
            .map_err(map_role_decision_error)?;
        self.project(events).await
    }

    pub async fn role(&self, role_id: RoleId) -> Result<RoleState, AuthzError> {
        Ok(self.role_index.role(role_id).await?)
    }

    pub async fn roles(
        &self,
        authority: Authority,
        actor: &Actor,
    ) -> Result<HashSet<RoleId>, AuthzError> {
        if !matches!(authority.actor(), Actor::System) {
            return Err(AuthzError::RoleRequiresSystemAuthority);
        }
        Ok(self.role_index.roles(actor).await?)
    }

    pub async fn all_roles(&self) -> Result<Vec<(RoleId, RoleState)>, AuthzError> {
        Ok(self.role_index.all_roles().await?)
    }

    async fn project(
        &self,
        events: Vec<PersistedEvent<PgEventId, AuthzEvent>>,
    ) -> Result<(), AuthzError> {
        Ok(self.role_index.project(events).await?)
    }
}

fn map_role_decision_error(error: DecisionError<RoleDecisionError>) -> AuthzError {
    match error {
        DecisionError::Domain(error) => error.into(),
        error => AuthzError::Disintegrate(error.to_string()),
    }
}

fn map_grant_decision_error(error: DecisionError<GrantDecisionError>) -> AuthzError {
    match error {
        DecisionError::Domain(error) => error.into(),
        error => AuthzError::Disintegrate(error.to_string()),
    }
}
