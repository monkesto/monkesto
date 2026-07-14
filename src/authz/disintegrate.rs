#![allow(dead_code)]

use super::role::RoleState;
use super::{GrantId, RoleId};
use crate::authority::{Actor, Authority};
use crate::name::Name;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use disintegrate::serde::messagepack::MessagePack;
use disintegrate::{
    Decision, Event, EventListener, PersistedEvent, StateMutate, StateQuery, StreamQuery, query,
};
use disintegrate_postgres::{
    PgDecisionMaker, PgEventId, PgEventListener, PgEventListenerConfig, PgEventListenerError,
    PgEventStore, PgSnapshotter, RetryAction, WithPgSnapshot, decision_maker,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::collections::HashSet;
use std::time::Duration;
use thiserror::Error;

#[derive(Clone, Debug, PartialEq, Event, Serialize, Deserialize)]
#[stream(RoleEvent, [RoleCreated, RoleActorAdded, RoleActorRemoved])]
#[stream(GrantEvent, [GrantCreated, GrantRevoked])]
pub(super) enum AuthzEvent {
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

#[derive(Clone, Debug, StateQuery, Serialize, Deserialize)]
#[state_query(RoleEvent)]
pub(super) struct Role {
    #[id]
    role_id: RoleId,
    name: Option<Name>,
    actors: HashSet<Actor>,
}

impl Role {
    fn new(role_id: RoleId) -> Self {
        Self {
            role_id,
            name: None,
            actors: HashSet::new(),
        }
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

#[derive(Clone, Debug, StateQuery, Serialize, Deserialize)]
#[state_query(GrantEvent)]
pub(super) struct Grant {
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

#[derive(Debug, thiserror::Error, PartialEq)]
pub(super) enum AuthzDecisionError {
    #[error("role already exists: {0}")]
    RoleExists(RoleId),
    #[error("role not found: {0}")]
    RoleNotFound(RoleId),
    #[error("grant already exists: {0}")]
    GrantExists(GrantId),
    #[error("grant not found: {0}")]
    GrantNotFound(GrantId),
}

pub(super) struct CreateRole {
    pub role_id: RoleId,
    pub name: Name,
    pub authority: Authority,
    pub timestamp: DateTime<Utc>,
}
impl Decision for CreateRole {
    type Event = AuthzEvent;
    type StateQuery = Role;
    type Error = AuthzDecisionError;
    fn state_query(&self) -> Role {
        Role::new(self.role_id)
    }
    fn process(&self, role: &Role) -> Result<Vec<AuthzEvent>, Self::Error> {
        if role.name.is_some() {
            return Err(AuthzDecisionError::RoleExists(self.role_id));
        }
        Ok(vec![AuthzEvent::RoleCreated {
            role_id: self.role_id,
            name: self.name.clone(),
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

pub(super) struct ChangeRoleActor {
    pub role_id: RoleId,
    pub actor: Actor,
    pub add: bool,
    pub authority: Authority,
    pub timestamp: DateTime<Utc>,
}
impl Decision for ChangeRoleActor {
    type Event = AuthzEvent;
    type StateQuery = Role;
    type Error = AuthzDecisionError;
    fn state_query(&self) -> Role {
        Role::new(self.role_id)
    }
    fn process(&self, role: &Role) -> Result<Vec<AuthzEvent>, Self::Error> {
        if role.name.is_none() {
            return Err(AuthzDecisionError::RoleNotFound(self.role_id));
        }
        if role.actors.contains(&self.actor) == self.add {
            return Ok(Vec::new());
        }
        Ok(vec![if self.add {
            AuthzEvent::RoleActorAdded {
                role_id: self.role_id,
                actor: self.actor.clone(),
                authority: self.authority.clone(),
                timestamp: self.timestamp,
            }
        } else {
            AuthzEvent::RoleActorRemoved {
                role_id: self.role_id,
                actor: self.actor.clone(),
                authority: self.authority.clone(),
                timestamp: self.timestamp,
            }
        }])
    }
}

pub(super) struct CreateGrant {
    pub grant_id: GrantId,
    pub role_id: RoleId,
    pub authority: Authority,
    pub timestamp: DateTime<Utc>,
}
impl Decision for CreateGrant {
    type Event = AuthzEvent;
    type StateQuery = (Role, Grant);
    type Error = AuthzDecisionError;
    fn state_query(&self) -> Self::StateQuery {
        (Role::new(self.role_id), Grant::new(self.grant_id))
    }
    fn process(&self, (role, grant): &Self::StateQuery) -> Result<Vec<AuthzEvent>, Self::Error> {
        if role.name.is_none() {
            return Err(AuthzDecisionError::RoleNotFound(self.role_id));
        }
        if grant.found {
            return Err(AuthzDecisionError::GrantExists(self.grant_id));
        }
        Ok(vec![AuthzEvent::GrantCreated {
            grant_id: self.grant_id,
            role_id: self.role_id,
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

pub(super) struct RevokeGrant {
    pub grant_id: GrantId,
    pub authority: Authority,
    pub timestamp: DateTime<Utc>,
}
impl Decision for RevokeGrant {
    type Event = AuthzEvent;
    type StateQuery = Grant;
    type Error = AuthzDecisionError;
    fn state_query(&self) -> Grant {
        Grant::new(self.grant_id)
    }
    fn process(&self, grant: &Grant) -> Result<Vec<AuthzEvent>, Self::Error> {
        if !grant.found {
            return Err(AuthzDecisionError::GrantNotFound(self.grant_id));
        }
        if grant.revoked {
            return Ok(Vec::new());
        }
        Ok(vec![AuthzEvent::GrantRevoked {
            grant_id: self.grant_id,
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

type PgAuthzDecisionMaker = PgDecisionMaker<AuthzEvent, MessagePack<AuthzEvent>, WithPgSnapshot>;
type PgAuthzEventStore = PgEventStore<AuthzEvent, MessagePack<AuthzEvent>>;

#[derive(Debug, Error)]
pub enum AuthzConnectError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error("disintegrate error: {0}")]
    Disintegrate(String),
}

#[derive(Clone)]
pub struct AuthzEventStore {
    event_store: PgAuthzEventStore,
    decision_maker: PgAuthzDecisionMaker,
}

#[derive(Clone)]
pub struct AuthzProjection {
    pool: PgPool,
    query: StreamQuery<PgEventId, AuthzEvent>,
}

#[derive(Clone)]
pub struct AuthzService {
    event_store: AuthzEventStore,
    projection: AuthzProjection,
}

#[derive(Debug, Error)]
pub(super) enum AuthzError {
    #[error(transparent)]
    Decision(#[from] AuthzDecisionError),
    #[error("disintegrate error: {0}")]
    Disintegrate(String),
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error(transparent)]
    Postcard(#[from] postcard::Error),
    #[error(transparent)]
    Ident(#[from] crate::id::IdentError),
    #[error("role lookup requires system authority")]
    RoleRequiresSystemAuthority,
}

impl AuthzEventStore {
    pub async fn try_new(pool: PgPool) -> Result<Self, AuthzConnectError> {
        let event_store = PgEventStore::try_new(pool.clone(), MessagePack::<AuthzEvent>::default())
            .await
            .map_err(|error| AuthzConnectError::Disintegrate(error.to_string()))?;
        let snapshotter = PgSnapshotter::try_new(pool.clone(), 10)
            .await
            .map_err(|error| AuthzConnectError::Disintegrate(error.to_string()))?;
        let decision_maker = decision_maker(event_store.clone(), WithPgSnapshot::new(snapshotter));
        Ok(Self {
            event_store,
            decision_maker,
        })
    }
}

impl AuthzProjection {
    pub async fn try_new(pool: PgPool, event_store: AuthzEventStore) -> Result<Self, sqlx::Error> {
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS authz_role (
            id BYTEA PRIMARY KEY, name BYTEA NOT NULL, latest_event_id BIGINT NOT NULL
        )"#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"CREATE TABLE IF NOT EXISTS authz_role_actor (
            role_id BYTEA NOT NULL, actor BYTEA NOT NULL, PRIMARY KEY (role_id, actor)
        )"#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS authz_role_actor_actor_idx ON authz_role_actor (actor)",
        )
        .execute(&pool)
        .await?;
        let projection = Self {
            pool,
            query: query!(AuthzEvent),
        };
        tokio::spawn(projection.clone().listen(event_store));
        Ok(projection)
    }

    pub async fn listen(self, event_store: AuthzEventStore) {
        PgEventListener::builder(event_store.event_store)
            .register_listener(
                self,
                PgEventListenerConfig::poller(Duration::from_secs(60))
                    .with_notifier()
                    .fetch_size(100)
                    .with_retry(|error: PgEventListenerError<AuthzError>, _| {
                        axum_login::tracing::error!(?error, "authz read model listener failed");
                        RetryAction::Abort
                    }),
            )
            .start_with_shutdown(crate::shutdown())
            .await
            .expect("authz event listener failed");
    }
}

impl AuthzService {
    pub fn new(event_store: AuthzEventStore, projection: AuthzProjection) -> Self {
        Self {
            event_store,
            projection,
        }
    }

    pub(super) async fn create_role(
        &self,
        authority: Authority,
        name: Name,
    ) -> Result<RoleId, AuthzError> {
        let role_id = RoleId::new();
        let events = self
            .event_store
            .decision_maker
            .make(CreateRole {
                role_id,
                name,
                authority,
                timestamp: Utc::now(),
            })
            .await
            .map_err(map_decision_error)?;
        self.project(events).await?;
        Ok(role_id)
    }

    pub(super) async fn grant_role(
        &self,
        authority: Authority,
        role_id: RoleId,
    ) -> Result<GrantId, AuthzError> {
        let grant_id = GrantId::new();
        let events = self
            .event_store
            .decision_maker
            .make(CreateGrant {
                grant_id,
                role_id,
                authority,
                timestamp: Utc::now(),
            })
            .await
            .map_err(map_decision_error)?;
        self.project(events).await?;
        Ok(grant_id)
    }

    pub(super) async fn revoke_grant(
        &self,
        authority: Authority,
        grant_id: GrantId,
    ) -> Result<(), AuthzError> {
        let events = self
            .event_store
            .decision_maker
            .make(RevokeGrant {
                grant_id,
                authority,
                timestamp: Utc::now(),
            })
            .await
            .map_err(map_decision_error)?;
        self.project(events).await
    }

    pub(super) async fn add_role_actor(
        &self,
        authority: Authority,
        role_id: RoleId,
        actor: Actor,
    ) -> Result<(), AuthzError> {
        self.change_role_actor(authority, role_id, actor, true)
            .await
    }

    pub(super) async fn remove_role_actor(
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
            .make(ChangeRoleActor {
                role_id,
                actor,
                add,
                authority,
                timestamp: Utc::now(),
            })
            .await
            .map_err(map_decision_error)?;
        self.project(events).await
    }

    pub(super) async fn role(&self, role_id: RoleId) -> Result<RoleState, AuthzError> {
        self.projection.role(role_id).await
    }

    pub(super) async fn roles(
        &self,
        authority: Authority,
        actor: &Actor,
    ) -> Result<HashSet<RoleId>, AuthzError> {
        if !matches!(authority.actor(), Actor::System) {
            return Err(AuthzError::RoleRequiresSystemAuthority);
        }
        self.projection.roles(actor).await
    }

    pub(super) async fn all_roles(&self) -> Result<Vec<(RoleId, RoleState)>, AuthzError> {
        self.projection.all_roles().await
    }

    async fn project(
        &self,
        events: Vec<PersistedEvent<PgEventId, AuthzEvent>>,
    ) -> Result<(), AuthzError> {
        self.projection.project(events).await
    }
}

impl AuthzProjection {
    async fn role(&self, role_id: RoleId) -> Result<RoleState, AuthzError> {
        let row = sqlx::query("SELECT name, latest_event_id FROM authz_role WHERE id = $1")
            .bind(role_id)
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Ok(RoleState::Absent);
        };
        let actors = sqlx::query("SELECT actor FROM authz_role_actor WHERE role_id = $1")
            .bind(role_id)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| postcard::from_bytes::<Actor>(&row.get::<Vec<u8>, _>("actor")))
            .collect::<Result<HashSet<_>, _>>()?;
        Ok(RoleState::Present {
            name: postcard::from_bytes(&row.get::<Vec<u8>, _>("name"))?,
            actors,
        })
    }

    async fn roles(&self, actor: &Actor) -> Result<HashSet<RoleId>, AuthzError> {
        sqlx::query("SELECT role_id FROM authz_role_actor WHERE actor = $1")
            .bind(postcard::to_allocvec(actor)?)
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .map(|row| {
                RoleId::try_from(row.get::<Vec<u8>, _>("role_id").as_slice()).map_err(Into::into)
            })
            .collect()
    }

    async fn all_roles(&self) -> Result<Vec<(RoleId, RoleState)>, AuthzError> {
        let rows = sqlx::query("SELECT id FROM authz_role ORDER BY id")
            .fetch_all(&self.pool)
            .await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let id = RoleId::try_from(row.get::<Vec<u8>, _>("id").as_slice())?;
            result.push((id, self.role(id).await?));
        }
        Ok(result)
    }

    async fn project(
        &self,
        events: Vec<PersistedEvent<PgEventId, AuthzEvent>>,
    ) -> Result<(), AuthzError> {
        for event in events {
            self.apply(event).await?;
        }
        Ok(())
    }

    async fn apply(&self, event: PersistedEvent<PgEventId, AuthzEvent>) -> Result<(), AuthzError> {
        let event_id = event.id();
        match event.into_inner() {
            AuthzEvent::RoleCreated { role_id, name, .. } => {
                sqlx::query("INSERT INTO authz_role (id, name, latest_event_id) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING")
                    .bind(role_id).bind(postcard::to_allocvec(&name)?).bind(event_id).execute(&self.pool).await?;
            }
            AuthzEvent::RoleActorAdded { role_id, actor, .. } => {
                let mut tx = self.pool.begin().await?;
                sqlx::query("INSERT INTO authz_role_actor (role_id, actor) VALUES ($1, $2) ON CONFLICT DO NOTHING")
                    .bind(role_id).bind(postcard::to_allocvec(&actor)?).execute(&mut *tx).await?;
                sqlx::query("UPDATE authz_role SET latest_event_id = GREATEST(latest_event_id, $1) WHERE id = $2")
                    .bind(event_id).bind(role_id).execute(&mut *tx).await?;
                tx.commit().await?;
            }
            AuthzEvent::RoleActorRemoved { role_id, actor, .. } => {
                let mut tx = self.pool.begin().await?;
                sqlx::query("DELETE FROM authz_role_actor WHERE role_id = $1 AND actor = $2")
                    .bind(role_id)
                    .bind(postcard::to_allocvec(&actor)?)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query("UPDATE authz_role SET latest_event_id = GREATEST(latest_event_id, $1) WHERE id = $2")
                    .bind(event_id).bind(role_id).execute(&mut *tx).await?;
                tx.commit().await?;
            }
            AuthzEvent::GrantCreated { .. } | AuthzEvent::GrantRevoked { .. } => {}
        }
        Ok(())
    }
}

#[async_trait]
impl EventListener<PgEventId, AuthzEvent> for AuthzProjection {
    type Error = AuthzError;

    fn id(&self) -> &'static str {
        "authz_roles"
    }

    fn query(&self) -> &StreamQuery<PgEventId, AuthzEvent> {
        &self.query
    }

    async fn handle(
        &self,
        event: PersistedEvent<PgEventId, AuthzEvent>,
    ) -> Result<(), Self::Error> {
        self.apply(event).await
    }
}

fn map_decision_error(error: disintegrate::DecisionError<AuthzDecisionError>) -> AuthzError {
    match error {
        disintegrate::DecisionError::Domain(error) => error.into(),
        error => AuthzError::Disintegrate(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authority() -> Authority {
        Authority::Direct(Actor::System)
    }
    fn name() -> Name {
        Name::try_new("Administrator".into()).expect("valid name")
    }

    #[test]
    fn creating_a_role_emits_role_created() {
        let role_id = RoleId::new();
        let decision = CreateRole {
            role_id,
            name: name(),
            authority: authority(),
            timestamp: Utc::now(),
        };
        let events = decision
            .process(&decision.state_query())
            .expect("valid decision");
        assert!(
            matches!(&events[..], [AuthzEvent::RoleCreated { role_id: id, .. }] if *id == role_id)
        );
    }

    #[test]
    fn adding_an_existing_actor_is_idempotent() {
        let role_id = RoleId::new();
        let actor = Actor::System;
        let mut role = Role::new(role_id);
        role.name = Some(name());
        role.actors.insert(actor.clone());
        let decision = ChangeRoleActor {
            role_id,
            actor,
            add: true,
            authority: authority(),
            timestamp: Utc::now(),
        };
        assert!(decision.process(&role).expect("valid decision").is_empty());
    }

    #[test]
    fn granting_a_missing_role_fails() {
        let role_id = RoleId::new();
        let decision = CreateGrant {
            grant_id: GrantId::new(),
            role_id,
            authority: authority(),
            timestamp: Utc::now(),
        };
        assert_eq!(
            decision.process(&decision.state_query()),
            Err(AuthzDecisionError::RoleNotFound(role_id))
        );
    }

    #[test]
    fn revoking_a_missing_grant_fails() {
        let grant_id = GrantId::new();
        let decision = RevokeGrant {
            grant_id,
            authority: authority(),
            timestamp: Utc::now(),
        };
        assert_eq!(
            decision.process(&decision.state_query()),
            Err(AuthzDecisionError::GrantNotFound(grant_id))
        );
    }

    #[test]
    fn revoking_an_already_revoked_grant_is_idempotent() {
        let grant_id = GrantId::new();
        let grant = Grant {
            grant_id,
            found: true,
            revoked: true,
        };
        let decision = RevokeGrant {
            grant_id,
            authority: authority(),
            timestamp: Utc::now(),
        };
        assert!(decision.process(&grant).expect("valid decision").is_empty());
    }
}
