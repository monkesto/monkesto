use super::{RoleId, RoleIndexError, RoleState};
use crate::authority::Actor;
use crate::authz::event::AuthzEvent;
use crate::authz::store::AuthzEventStore;
use crate::name::Name;
use async_trait::async_trait;
use disintegrate::{EventListener, PersistedEvent, StreamQuery, query};
use disintegrate_postgres::{
    PgEventId, PgEventListener, PgEventListenerConfig, PgEventListenerError, RetryAction,
};
use sqlx::PgPool;
use std::collections::HashSet;
use std::time::Duration;

#[derive(Clone)]
pub struct RoleIndex {
    pool: PgPool,
    query: StreamQuery<PgEventId, AuthzEvent>,
}

impl RoleIndex {
    pub async fn try_new(pool: PgPool, event_store: AuthzEventStore) -> Result<Self, sqlx::Error> {
        sqlx::query!(
            r#"CREATE TABLE IF NOT EXISTS authz_role (
            id TEXT PRIMARY KEY, name BYTEA NOT NULL, latest_event_id BIGINT NOT NULL
        )"#,
        )
        .execute(&pool)
        .await?;
        sqlx::query!(
            r#"CREATE TABLE IF NOT EXISTS authz_role_actor (
            role_id TEXT NOT NULL, actor BYTEA NOT NULL, PRIMARY KEY (role_id, actor)
        )"#,
        )
        .execute(&pool)
        .await?;
        sqlx::query!(
            "CREATE INDEX IF NOT EXISTS authz_role_actor_actor_idx ON authz_role_actor (actor)",
        )
        .execute(&pool)
        .await?;
        let index = Self {
            pool,
            query: query!(AuthzEvent),
        };
        tokio::spawn(index.clone().listen(event_store));
        Ok(index)
    }

    async fn listen(self, event_store: AuthzEventStore) {
        PgEventListener::builder(event_store.event_store)
            .register_listener(
                self,
                PgEventListenerConfig::poller(Duration::from_secs(60))
                    .with_notifier()
                    .fetch_size(100)
                    .with_retry(|error: PgEventListenerError<RoleIndexError>, _| {
                        axum_login::tracing::error!(?error, "authz read model listener failed");
                        RetryAction::Abort
                    }),
            )
            .start_with_shutdown(crate::shutdown())
            .await
            .expect("authz event listener failed");
    }

    pub async fn find_role(&self, role_id: RoleId) -> Result<RoleState, RoleIndexError> {
        let name = sqlx::query_scalar!(
            r#"SELECT name as "name: Name" FROM authz_role WHERE id = $1"#,
            role_id as RoleId
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(name) = name {
            let actors = sqlx::query_scalar!(
                r#"SELECT actor as "actor: Actor" FROM authz_role_actor WHERE role_id = $1"#,
                role_id as RoleId
            )
            .fetch_all(&self.pool)
            .await?
            .into_iter()
            .collect::<HashSet<_>>();

            return Ok(RoleState::Present { name, actors });
        }

        Ok(RoleState::Absent)
    }

    pub async fn find_roles(&self, actor: &Actor) -> Result<HashSet<RoleId>, RoleIndexError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT role_id as "role_id: RoleId" FROM authz_role_actor WHERE actor = $1"#,
            actor as &Actor
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .collect())
    }

    pub async fn find_all_roles(&self) -> Result<Vec<(RoleId, RoleState)>, RoleIndexError> {
        let roles = sqlx::query_scalar!(r#"SELECT id as "id: RoleId" FROM authz_role ORDER BY id"#)
            .fetch_all(&self.pool)
            .await?;
        let mut result = Vec::with_capacity(roles.len());
        for id in roles {
            result.push((id, self.find_role(id).await?));
        }
        Ok(result)
    }

    pub async fn apply_all(
        &self,
        events: Vec<PersistedEvent<PgEventId, AuthzEvent>>,
    ) -> Result<(), RoleIndexError> {
        for event in events {
            self.apply(event).await?;
        }
        Ok(())
    }

    async fn apply(
        &self,
        event: PersistedEvent<PgEventId, AuthzEvent>,
    ) -> Result<(), RoleIndexError> {
        let event_id = event.id();
        match event.into_inner() {
            AuthzEvent::RoleCreated { role_id, name, .. } => {
                sqlx::query!("INSERT INTO authz_role (id, name, latest_event_id) VALUES ($1, $2, $3) ON CONFLICT (id) DO NOTHING", role_id as RoleId, name as Name, event_id)
                    .execute(&self.pool).await?;
            }
            AuthzEvent::RoleActorAdded { role_id, actor, .. } => {
                let mut tx = self.pool.begin().await?;
                sqlx::query!("INSERT INTO authz_role_actor (role_id, actor) VALUES ($1, $2) ON CONFLICT DO NOTHING", role_id as RoleId, actor as Actor)
                    .execute(&mut *tx).await?;
                sqlx::query!("UPDATE authz_role SET latest_event_id = GREATEST(latest_event_id, $1) WHERE id = $2", event_id, role_id as RoleId)
                    .execute(&mut *tx).await?;
                tx.commit().await?;
            }
            AuthzEvent::RoleActorRemoved { role_id, actor, .. } => {
                let mut tx = self.pool.begin().await?;
                sqlx::query!(
                    "DELETE FROM authz_role_actor WHERE role_id = $1 AND actor = $2",
                    role_id as RoleId,
                    actor as Actor
                )
                .execute(&mut *tx)
                .await?;
                sqlx::query!("UPDATE authz_role SET latest_event_id = GREATEST(latest_event_id, $1) WHERE id = $2", event_id, role_id as RoleId)
                    .execute(&mut *tx).await?;
                tx.commit().await?;
            }
            AuthzEvent::GrantCreated { .. } | AuthzEvent::GrantRevoked { .. } => {}
        }
        Ok(())
    }
}

#[async_trait]
impl EventListener<PgEventId, AuthzEvent> for RoleIndex {
    type Error = RoleIndexError;

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
