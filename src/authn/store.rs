use super::AuthnEvent;
use disintegrate::serde::messagepack::MessagePack;
use disintegrate_postgres::PgEventStore;
use sqlx::PgPool;

pub(super) type PgAuthnEventStore = PgEventStore<AuthnEvent, MessagePack<AuthnEvent>>;

#[derive(Clone)]
pub struct AuthnEventStore {
    pub(super) event_store: PgAuthnEventStore,
}

impl AuthnEventStore {
    pub async fn try_new(pool: PgPool) -> Result<Self, disintegrate_postgres::Error> {
        let event_store = PgEventStore::try_new(pool, MessagePack::<AuthnEvent>::default()).await?;
        Ok(Self { event_store })
    }
}
