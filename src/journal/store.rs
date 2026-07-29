use crate::journal::domain::JournalDomainEvent;
use crate::proto::event::journal::ProtoJournalDomainEvent;
use disintegrate::serde::prost::Prost;
use disintegrate_postgres::PgEventStore;
use sqlx::PgPool;

pub type PgJournalEventStore =
    PgEventStore<JournalDomainEvent, Prost<JournalDomainEvent, ProtoJournalDomainEvent>>;

#[derive(Clone)]
pub struct JournalEventStore {
    pub event_store: PgJournalEventStore,
}

impl JournalEventStore {
    pub async fn try_new(pool: PgPool) -> Result<Self, disintegrate_postgres::Error> {
        let event_store = PgEventStore::try_new(
            pool,
            Prost::<JournalDomainEvent, ProtoJournalDomainEvent>::default(),
        )
        .await?;
        Ok(Self { event_store })
    }
}
