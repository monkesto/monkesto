use super::{RoleId, RoleIndexError, RoleState};
use crate::authority::Actor;
use crate::authz::event::AuthzEvent;
use crate::authz::store::AuthzEventStore;
use async_trait::async_trait;
use disintegrate::{EventListener, PersistedEvent, StreamQuery, query};
use disintegrate_postgres::{
    PgEventId, PgEventListener, PgEventListenerConfig, PgEventListenerError, RetryAction,
};
use sqlx::{PgPool, Row};
use std::collections::HashSet;
use std::time::Duration;

#[derive(Clone)]
pub struct RoleIndex {
    pool: PgPool,
    query: StreamQuery<PgEventId, AuthzEvent>,
}

impl RoleIndex {
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

    pub async fn find_roles(&self, actor: &Actor) -> Result<HashSet<RoleId>, RoleIndexError> {
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

    pub async fn find_all_roles(&self) -> Result<Vec<(RoleId, RoleState)>, RoleIndexError> {
        let rows = sqlx::query("SELECT id FROM authz_role ORDER BY id")
            .fetch_all(&self.pool)
            .await?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let id = RoleId::try_from(row.get::<Vec<u8>, _>("id").as_slice())?;
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
