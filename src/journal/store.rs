use crate::journal::domain::JournalDomainEvent;
use disintegrate::serde::messagepack::MessagePack;
use disintegrate_postgres::PgEventStore;
use sqlx::PgPool;

pub type PgJournalEventStore = PgEventStore<JournalDomainEvent, MessagePack<JournalDomainEvent>>;

#[derive(Clone)]
pub struct JournalEventStore {
    pub event_store: PgJournalEventStore,
}

impl JournalEventStore {
    pub async fn try_new(pool: PgPool) -> Result<Self, disintegrate_postgres::Error> {
        let event_store =
            PgEventStore::try_new(pool, MessagePack::<JournalDomainEvent>::default()).await?;
        Ok(Self { event_store })
    }
}
