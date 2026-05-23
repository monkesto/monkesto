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
use std::sync::Arc;
use std::sync::Mutex;

struct MemoryState<E>
where
    E: EventFamily,
{
    latest: EventId,
    events: Vec<E>,
    by_id: HashMap<E::Id, Vec<E>>,
}

pub struct MemoryStore<E>
where
    E: EventFamily,
{
    state: Arc<Mutex<MemoryState<E>>>,
}

impl<E> MemoryStore<E>
where
    E: EventFamily,
{
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(MemoryState {
                latest: EventId::from(0),
                events: Vec::new(),
                by_id: HashMap::new(),
            })),
        }
    }
}

impl<E> Default for MemoryStore<E>
where
    E: EventFamily,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E> Store<E> for MemoryStore<E>
where
    E: EventFamily,
    E::Record: RecordFor<E>,
{
    type Error = Infallible;

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
                .by_id
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
                .by_id
                .entry(event.id())
                .or_default()
                .push(event.clone());
            events.push(event);
        }

        state.events.extend(events.iter().cloned());
        Ok(Outcome::Recorded(events))
    }

    async fn review(
        &self,
        id: E::Id,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<E>, Self::Error> {
        let state = self.state.lock().expect("poisoned");
        let events = state.by_id.get(&id).into_iter().flatten().cloned();
        Ok(page_from_iter(events, after, limit, state.latest))
    }

    #[rustfmt::skip]
    async fn observe(
        &self,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<E>, Self::Error> {
        let state = self.state.lock().expect("poisoned");
        Ok(page_from_iter(
            state.events.iter().cloned(),
            after,
            limit,
            state.latest,
        ))
    }
}

pub fn page_from_iter<E>(
    events: impl IntoIterator<Item = E>,
    after: After<EventId>,
    limit: usize,
    latest: EventId,
) -> Page<E>
where
    E: EventFamily,
{
    let filtered: Vec<E> = events
        .into_iter()
        .filter(|event| match after {
            After::Start => true,
            After::Specific(event_id) => event.event_id() > event_id,
        })
        .take(limit + 1)
        .collect();
    let more = filtered.len() > limit;
    let items: Vec<E> = filtered.into_iter().take(limit).collect();
    let next = items.last().map(EventFamily::event_id).unwrap_or(latest);
    Page { items, more, next }
}
