use super::event::AuthzEvent;
use disintegrate::serde::messagepack::MessagePack;
use disintegrate_postgres::{
    PgDecisionMaker, PgEventStore, PgSnapshotter, WithPgSnapshot, decision_maker,
};
use sqlx::PgPool;
use thiserror::Error;

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
    pub event_store: PgAuthzEventStore,
    pub decision_maker: PgAuthzDecisionMaker,
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
