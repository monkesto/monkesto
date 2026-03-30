use crate::authority::Authority;
use crate::store::After;
use crate::store::Event;
use crate::store::EventId;
use crate::store::Outcome;
use crate::store::Page;
use crate::store::Select;
use crate::store::Store;
use crate::store::When;
use chrono::DateTime;
use chrono::Utc;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::hash::Hash;
use std::sync::Arc;
use std::sync::Mutex;

#[expect(dead_code)]
#[derive(Debug)]
pub enum MemoryStoreError {}

impl fmt::Display for MemoryStoreError {
    fn fmt(&self, _: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

impl Error for MemoryStoreError {}

#[expect(dead_code)]
struct MemoryStoreState<I, P>
where
    I: Send + Sync + Copy + Clone + Eq + Hash + Sized,
    P: Send + Sync + Clone,
{
    latest: EventId,
    events: Vec<Event<I, P>>,
    select_events: HashMap<I, Vec<Event<I, P>>>,
}

#[expect(dead_code)]
struct MemoryStore<I, P>
where
    I: Send + Sync + Copy + Clone + Eq + Hash + Sized,
    P: Send + Sync + Clone,
{
    state: Arc<Mutex<MemoryStoreState<I, P>>>,
}

impl<I, P> Default for MemoryStore<I, P>
where
    I: Send + Sync + Copy + Clone + Eq + Hash + Sized,
    P: Send + Sync + Clone,
{
    fn default() -> Self {
        Self {
            state: Arc::new(Mutex::new(MemoryStoreState {
                latest: EventId(0),
                events: Vec::new(),
                select_events: HashMap::new(),
            })),
        }
    }
}

impl<I, P> Store for MemoryStore<I, P>
where
    I: Send + Sync + Copy + Clone + Eq + Hash + Sized,
    P: Send + Sync + Clone,
{
    type Id = I;
    type Payload = P;
    type Error = MemoryStoreError;

    async fn record(
        &self,
        by: Authority,
        at: DateTime<Utc>,
        id: Self::Id,
        payload: Self::Payload,
        when: When<EventId>,
    ) -> Result<Outcome<I, P>, Self::Error> {
        let mut state = self.state.lock().expect("poisoned");
        match when {
            When::Always => {}
            When::Current(event_id) => {
                let condition_met = state
                    .select_events
                    .get(&id)
                    .and_then(|v| v.last())
                    .map(|e| e.event_id <= event_id)
                    .unwrap_or(false);
                if !condition_met {
                    return Ok(Outcome::Skipped);
                }
            }
        }
        let event = Event {
            event_id: state.latest.next(),
            timestamp: at,
            authority: by,
            id,
            payload,
        };
        state.events.push(event.clone());
        state
            .select_events
            .entry(id)
            .or_default()
            .push(event.clone());
        Ok(Outcome::Recorded(event))
    }

    async fn review(
        &self,
        select: Select<Self::Id>,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<Self::Id, Self::Payload>, Self::Error> {
        let state = self.state.lock().expect("poisoned");
        let full = match select {
            Select::All => state.events.clone(),
            Select::One(id) => state.select_events.get(&id).cloned().unwrap_or_default(),
        };
        let filtered: Vec<Event<I, P>> = match after {
            After::Start => full.into_iter().take(limit + 1).collect(),
            After::Specific(event_id) => full
                .into_iter()
                .filter(|e| e.event_id > event_id)
                .take(limit + 1)
                .collect(),
        };
        let items: Vec<Event<I, P>> = filtered.clone().into_iter().take(limit).collect();
        let more = filtered.len() > limit;
        let next = items.last().map(|e| e.event_id).unwrap_or(state.latest);
        Ok(Page { items, more, next })
    }
}
