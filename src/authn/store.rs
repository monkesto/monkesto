use super::AuthnEvent;
use disintegrate::serde::prost::Prost;
use disintegrate_postgres::PgEventStore;
use proto::event::authn::ProtoAuthnEvent;
use sqlx::PgPool;

pub(super) type PgAuthnEventStore = PgEventStore<AuthnEvent, Prost<AuthnEvent, ProtoAuthnEvent>>;

#[derive(Clone)]
pub struct AuthnEventStore {
    pub(super) event_store: PgAuthnEventStore,
}

impl AuthnEventStore {
    pub async fn try_new(pool: PgPool) -> Result<Self, disintegrate_postgres::Error> {
        let event_store =
            PgEventStore::try_new(pool, Prost::<AuthnEvent, ProtoAuthnEvent>::default()).await?;
        Ok(Self { event_store })
    }
}
