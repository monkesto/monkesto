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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::revised::Event;
    use crate::store::revised::EventFamily;
    use crate::store::revised::EventId;
    use crate::store::revised::Outcome;
    use crate::store::revised::Record;
    use crate::store::revised::RecordFor;
    use crate::store::revised::Stream;
    use crate::store::revised::When;
    use chrono::Utc;

    revised_store_contract_tests!(MemoryStore::<TestEvent>::new());

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

    #[derive(Clone)]
    enum TestRecord {
        Alpha(Record<AlphaStream>),
        Beta(Record<BetaStream>),
    }

    #[derive(Clone, Debug)]
    enum TestEvent {
        Alpha(Event<i32, AlphaStream>),
        Beta(Event<i32, BetaStream>),
    }

    impl EventFamily for TestEvent {
        type Id = TestId;
        type Record = TestRecord;
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

    impl RecordFor<TestEvent> for TestRecord {
        fn id(&self) -> TestId {
            match self {
                TestRecord::Alpha(record) => TestId::Alpha(record.id),
                TestRecord::Beta(record) => TestId::Beta(record.id),
            }
        }

        fn when(&self) -> When<EventId> {
            match self {
                TestRecord::Alpha(record) => record.when,
                TestRecord::Beta(record) => record.when,
            }
        }

        fn into_event(
            self,
            event_id: EventId,
            authority: i32,
            timestamp: chrono::DateTime<chrono::Utc>,
        ) -> TestEvent {
            match self {
                TestRecord::Alpha(record) => TestEvent::Alpha(Event {
                    event_id,
                    timestamp,
                    authority,
                    id: record.id,
                    payload: record.payload,
                }),
                TestRecord::Beta(record) => TestEvent::Beta(Event {
                    event_id,
                    timestamp,
                    authority,
                    id: record.id,
                    payload: record.payload,
                }),
            }
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
            .record(
                1,
                Utc::now(),
                TestRecord::Alpha(Record {
                    id: 10,
                    payload: "first",
                    when: When::Empty,
                }),
            )
            .await
            .expect("record should succeed");
        store
            .record(
                1,
                Utc::now(),
                TestRecord::Alpha(Record {
                    id: 10,
                    payload: "skipped",
                    when: When::Empty,
                }),
            )
            .await
            .expect("record should succeed");

        let events = projection_events.lock().expect("poisoned");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], TestEvent::Alpha(event) if event.payload == "first"));
    }

    #[tokio::test]
    async fn projection_receives_all_commit_events_before_return() {
        let projection = TestProjection::default();
        let projection_events = projection.events.clone();
        let store = MemoryStore::<TestEvent, TestProjection>::with_projection(projection);
        let result = store
            .commit(
                1,
                Utc::now(),
                vec![
                    TestRecord::Alpha(Record {
                        id: 10,
                        payload: "alpha",
                        when: When::Empty,
                    }),
                    TestRecord::Beta(Record {
                        id: 20,
                        payload: "beta",
                        when: When::Empty,
                    }),
                ],
            )
            .await
            .expect("commit should succeed");

        let Outcome::Recorded(recorded_events) = result else {
            panic!("expected recorded");
        };
        let projected_events = projection_events.lock().expect("poisoned");
        assert_eq!(projected_events.len(), 2);
        assert_eq!(
            projected_events[0].event_id(),
            recorded_events[0].event_id()
        );
        assert_eq!(
            projected_events[1].event_id(),
            recorded_events[1].event_id()
        );
    }
}
