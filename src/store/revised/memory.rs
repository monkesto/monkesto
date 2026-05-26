use crate::store::revised::After;
use crate::store::revised::EventFamily;
use crate::store::revised::EventId;
use crate::store::revised::Outcome;
use crate::store::revised::Page;
use crate::store::revised::RecordFor;
use crate::store::revised::Store;
use crate::store::revised::When;
use std::collections::HashMap;
use std::convert::Infallible;
use std::error::Error;
use std::sync::Arc;
use std::sync::Mutex;

pub trait SyncMemoryProjection<E: EventFamily>: Send + Sync {
    type Error: Error + Send + Sync + 'static;

    fn apply(&mut self, events: &[E]) -> Result<(), Self::Error>;
}

#[derive(Default)]
pub struct NoProjection;

impl<E> SyncMemoryProjection<E> for NoProjection
where
    E: EventFamily,
{
    type Error = Infallible;

    fn apply(&mut self, _events: &[E]) -> Result<(), Self::Error> {
        Ok(())
    }
}

struct MemoryState<E, P>
where
    E: EventFamily,
{
    latest: EventId,
    events: Vec<E>,
    streams: HashMap<E::Id, Vec<E>>,
    projection: P,
}

pub struct MemoryStore<E, P = NoProjection>
where
    E: EventFamily,
{
    state: Arc<Mutex<MemoryState<E, P>>>,
}

impl<E, P> MemoryStore<E, P>
where
    E: EventFamily,
    P: Default,
{
    pub fn new() -> Self {
        Self::with_projection(P::default())
    }
}

impl<E, P> MemoryStore<E, P>
where
    E: EventFamily,
{
    pub fn with_projection(projection: P) -> Self {
        Self {
            state: Arc::new(Mutex::new(MemoryState {
                latest: EventId::from(0),
                events: Vec::new(),
                streams: HashMap::new(),
                projection,
            })),
        }
    }
}

impl<E, P> Default for MemoryStore<E, P>
where
    E: EventFamily,
    P: Default,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E, P> Store<E> for MemoryStore<E, P>
where
    E: EventFamily,
    E::Record: RecordFor<E>,
    P: SyncMemoryProjection<E>,
{
    type Error = P::Error;

    async fn record(
        &self,
        by: E::Authority,
        at: chrono::DateTime<chrono::Utc>,
        record: E::Record,
    ) -> Result<Outcome<E>, Self::Error> {
        self.commit(by, at, vec![record]).await
    }

    async fn commit(
        &self,
        by: E::Authority,
        at: chrono::DateTime<chrono::Utc>,
        records: Vec<E::Record>,
    ) -> Result<Outcome<E>, Self::Error> {
        let mut state = self.state.lock().expect("poisoned");
        if records.iter().any(|record| {
            let latest = state
                .streams
                .get(&record.id())
                .and_then(|events| events.last())
                .map(EventFamily::event_id);

            match record.when() {
                When::Empty => latest.is_some(),
                When::Within(event_id) => latest.map(|latest| latest > event_id).unwrap_or(true),
            }
        }) {
            return Ok(Outcome::Skipped);
        }

        let mut events = Vec::with_capacity(records.len());
        for record in records {
            state.latest = state.latest.next();
            let event_id = state.latest;
            let event = record.into_event(event_id, by.clone(), at);
            state
                .streams
                .entry(event.id())
                .or_default()
                .push(event.clone());
            events.push(event);
        }

        state.events.extend(events.iter().cloned());
        state.projection.apply(&events)?;
        Ok(Outcome::Recorded(events))
    }

    async fn review(
        &self,
        id: E::Id,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<E>, Self::Error> {
        let state = self.state.lock().expect("poisoned");
        let filtered: Vec<E> = state
            .streams
            .get(&id)
            .into_iter()
            .flatten()
            .filter(|event| match after {
                After::Start => true,
                After::Specific(event_id) => event.event_id() > event_id,
            })
            .take(limit + 1)
            .cloned()
            .collect();
        let more = filtered.len() > limit;
        let items: Vec<E> = filtered.into_iter().take(limit).collect();
        let next = items
            .last()
            .map(EventFamily::event_id)
            .unwrap_or(state.latest);
        Ok(Page { items, more, next })
    }

    #[rustfmt::skip]
    async fn observe(
        &self,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<E>, Self::Error> {
        let state = self.state.lock().expect("poisoned");
        let filtered: Vec<E> = state
            .events
            .iter()
            .filter(|event| match after {
                After::Start => true,
                After::Specific(event_id) => event.event_id() > event_id,
            })
            .take(limit + 1)
            .cloned()
            .collect();
        let more = filtered.len() > limit;
        let items: Vec<E> = filtered.into_iter().take(limit).collect();
        let next = items.last().map(EventFamily::event_id).unwrap_or(state.latest);
        Ok(Page { items, more, next })
    }
}
