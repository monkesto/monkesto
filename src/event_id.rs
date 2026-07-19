use disintegrate::PersistedEvent;
use disintegrate_postgres::PgEventId;

pub trait GetEventId {
    fn event_id(&self) -> PgEventId;
}

impl<T: disintegrate::Event> GetEventId for Vec<PersistedEvent<PgEventId, T>> {
    /// Returns the latest eventid in the set
    ///
    /// Safety: This function assumes that the set has at least one event,
    /// it will panic otherwise
    fn event_id(&self) -> PgEventId {
        self.last()
            .expect("the decision maker should always return at least one event")
            .id()
    }
}
