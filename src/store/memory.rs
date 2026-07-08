use super::After;
use super::EventFamily;
use super::EventFor;
use super::EventId;
use super::Outcome;
use super::Page;
use super::Store;
use super::When;
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
    P: SyncMemoryProjection<E>,
{
    type Error = P::Error;

    async fn record<S>(
        &self,
        by: E::Authority,
        at: chrono::DateTime<chrono::Utc>,
        id: S::Id,
        payload: S::Payload,
        when: When<EventId>,
    ) -> Result<Outcome<E>, Self::Error>
    where
        S: super::Stream,
        E: EventFor<S>,
    {
        let mut state = self.state.lock().expect("poisoned");
        let family_id = E::id_for(id);
        let latest = state
            .streams
            .get(&family_id)
            .and_then(|events| events.last())
            .map(EventFamily::event_id);

        let should_skip = match when {
            When::Empty => latest.is_some(),
            When::Within(event_id) => latest.map(|latest| latest > event_id).unwrap_or(true),
        };
        if should_skip {
            return Ok(Outcome::Skipped);
        }

        state.latest = state.latest.next();
        let event_id = state.latest;
        let event = E::new_event(event_id, by, at, id, payload);
        state
            .streams
            .entry(event.id())
            .or_default()
            .push(event.clone());

        state.events.push(event.clone());
        state.projection.apply(std::slice::from_ref(&event))?;
        Ok(Outcome::Recorded(event))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Event;
    use crate::store::EventFamily;
    use crate::store::EventFor;
    use crate::store::EventId;
    use crate::store::Stream;
    use crate::store::When;
    use chrono::Utc;

    store_contract_tests!(MemoryStore::<TestEvent>::new());

    #[derive(Clone, Copy, Debug)]
    struct AlphaStream;

    impl Stream for AlphaStream {
        type Id = u32;
        type Payload = &'static str;
    }

    #[derive(Clone, Copy, Debug)]
    struct BetaStream;

    impl Stream for BetaStream {
        type Id = u32;
        type Payload = &'static str;
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    enum TestId {
        Alpha(u32),
        Beta(u32),
    }

    #[derive(Clone, Debug)]
    enum TestEvent {
        Alpha(Event<i32, AlphaStream>),
        Beta(Event<i32, BetaStream>),
    }

    impl EventFamily for TestEvent {
        type Id = TestId;
        type Authority = i32;

        fn event_id(&self) -> EventId {
            match self {
                TestEvent::Alpha(event) => event.event_id,
                TestEvent::Beta(event) => event.event_id,
            }
        }

        fn id(&self) -> Self::Id {
            match self {
                TestEvent::Alpha(event) => TestId::Alpha(event.id),
                TestEvent::Beta(event) => TestId::Beta(event.id),
            }
        }
    }

    impl EventFor<AlphaStream> for TestEvent {
        fn id_for(id: u32) -> TestId {
            TestId::Alpha(id)
        }

        fn new_event(
            event_id: EventId,
            authority: i32,
            timestamp: chrono::DateTime<chrono::Utc>,
            id: u32,
            payload: &'static str,
        ) -> TestEvent {
            TestEvent::Alpha(Event {
                event_id,
                timestamp,
                authority,
                id,
                payload,
            })
        }
    }

    impl EventFor<BetaStream> for TestEvent {
        fn id_for(id: u32) -> TestId {
            TestId::Beta(id)
        }

        fn new_event(
            event_id: EventId,
            authority: i32,
            timestamp: chrono::DateTime<chrono::Utc>,
            id: u32,
            payload: &'static str,
        ) -> TestEvent {
            TestEvent::Beta(Event {
                event_id,
                timestamp,
                authority,
                id,
                payload,
            })
        }
    }

    #[derive(Clone, Default)]
    struct TestProjection {
        events: Arc<Mutex<Vec<TestEvent>>>,
    }

    impl SyncMemoryProjection<TestEvent> for TestProjection {
        type Error = Infallible;

        fn apply(&mut self, events: &[TestEvent]) -> Result<(), Self::Error> {
            self.events
                .lock()
                .expect("poisoned")
                .extend_from_slice(events);
            Ok(())
        }
    }

    #[tokio::test]
    async fn projection_receives_recorded_events_and_not_skipped_events() {
        let projection = TestProjection::default();
        let projection_events = projection.events.clone();
        let store = MemoryStore::<TestEvent, TestProjection>::with_projection(projection);
        store
            .record::<AlphaStream>(1, Utc::now(), 10, "first", When::Empty)
            .await
            .expect("record should succeed");
        store
            .record::<AlphaStream>(1, Utc::now(), 10, "skipped", When::Empty)
            .await
            .expect("record should succeed");

        let events = projection_events.lock().expect("poisoned");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], TestEvent::Alpha(event) if event.payload == "first"));
    }
}
