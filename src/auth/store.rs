use super::AuthEvent;
use disintegrate::serde::messagepack::MessagePack;
use disintegrate_postgres::PgEventStore;
use sqlx::PgPool;

pub(super) type PgAuthEventStore = PgEventStore<AuthEvent, MessagePack<AuthEvent>>;

#[derive(Clone)]
pub struct AuthEventStore {
    pub(super) event_store: PgAuthEventStore,
}

impl AuthEventStore {
    pub async fn try_new(pool: PgPool) -> Result<Self, disintegrate_postgres::Error> {
        let event_store = PgEventStore::try_new(pool, MessagePack::<AuthEvent>::default()).await?;
        Ok(Self { event_store })
    }
}
