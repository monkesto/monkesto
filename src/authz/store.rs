use super::event::AuthzEvent;
use disintegrate::serde::prost::Prost;
use disintegrate_postgres::{
    PgDecisionMaker, PgEventStore, PgSnapshotter, WithPgSnapshot, decision_maker,
};
use proto::event::authz::ProtoAuthzEvent;
use sqlx::PgPool;
use thiserror::Error;

type PgAuthzDecisionMaker =
    PgDecisionMaker<AuthzEvent, Prost<AuthzEvent, ProtoAuthzEvent>, WithPgSnapshot>;
type PgAuthzEventStore = PgEventStore<AuthzEvent, Prost<AuthzEvent, ProtoAuthzEvent>>;

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
        let event_store = PgEventStore::try_new(
            pool.clone(),
            Prost::<AuthzEvent, ProtoAuthzEvent>::default(),
        )
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
