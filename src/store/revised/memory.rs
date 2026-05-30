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
    use crate::store::revised::Record;
    use crate::store::revised::Stream;
    use chrono::Utc;

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
    async fn record_empty_no_prior() {
        let store = MemoryStore::<TestEvent>::new();
        let result = store
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

        assert!(matches!(result, Outcome::Recorded(_)));
    }

    #[tokio::test]
    async fn record_empty_with_prior_skips() {
        let store = MemoryStore::<TestEvent>::new();
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

        let result = store
            .record(
                1,
                Utc::now(),
                TestRecord::Alpha(Record {
                    id: 10,
                    payload: "second",
                    when: When::Empty,
                }),
            )
            .await
            .expect("record should succeed");

        assert!(matches!(result, Outcome::Skipped));
    }

    #[tokio::test]
    async fn record_empty_for_one_id_unaffected_by_other_id() {
        let store = MemoryStore::<TestEvent>::new();
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

        let result = store
            .record(
                1,
                Utc::now(),
                TestRecord::Alpha(Record {
                    id: 20,
                    payload: "second",
                    when: When::Empty,
                }),
            )
            .await
            .expect("record should succeed");

        assert!(matches!(result, Outcome::Recorded(_)));
    }

    #[tokio::test]
    async fn record_within_last_is_recorded() {
        let store = MemoryStore::<TestEvent>::new();
        let Outcome::Recorded(events) = store
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
            .expect("record should succeed")
        else {
            panic!("expected recorded");
        };
        let event_id = events[0].event_id();

        let result = store
            .record(
                1,
                Utc::now(),
                TestRecord::Alpha(Record {
                    id: 10,
                    payload: "second",
                    when: When::Within(event_id),
                }),
            )
            .await
            .expect("record should succeed");

        assert!(matches!(result, Outcome::Recorded(_)));
    }

    #[tokio::test]
    async fn record_within_old_event_id_skips() {
        let store = MemoryStore::<TestEvent>::new();
        let Outcome::Recorded(events) = store
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
            .expect("record should succeed")
        else {
            panic!("expected recorded");
        };
        let event_id = events[0].event_id();
        store
            .record(
                1,
                Utc::now(),
                TestRecord::Alpha(Record {
                    id: 10,
                    payload: "second",
                    when: When::Within(event_id),
                }),
            )
            .await
            .expect("record should succeed");

        let result = store
            .record(
                1,
                Utc::now(),
                TestRecord::Alpha(Record {
                    id: 10,
                    payload: "third",
                    when: When::Within(event_id),
                }),
            )
            .await
            .expect("record should succeed");

        assert!(matches!(result, Outcome::Skipped));
    }

    #[tokio::test]
    async fn review_returns_only_requested_stream_id() {
        let store = MemoryStore::<TestEvent>::new();
        store
            .commit(
                1,
                Utc::now(),
                vec![
                    TestRecord::Alpha(Record {
                        id: 10,
                        payload: "alpha-a",
                        when: When::Empty,
                    }),
                    TestRecord::Alpha(Record {
                        id: 20,
                        payload: "alpha-b",
                        when: When::Empty,
                    }),
                    TestRecord::Beta(Record {
                        id: 10,
                        payload: "beta-a",
                        when: When::Empty,
                    }),
                ],
            )
            .await
            .expect("commit should succeed");

        let page = store
            .review(TestId::Alpha(10), After::Start, 10)
            .await
            .expect("review should succeed");

        assert_eq!(page.items.len(), 1);
        assert!(matches!(&page.items[0], TestEvent::Alpha(event) if event.payload == "alpha-a"));
    }

    #[tokio::test]
    async fn review_respects_after_and_limit() {
        let store = MemoryStore::<TestEvent>::new();
        let Outcome::Recorded(events) = store
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
            .expect("record should succeed")
        else {
            panic!("expected recorded");
        };
        let first_id = events[0].event_id();
        store
            .record(
                1,
                Utc::now(),
                TestRecord::Alpha(Record {
                    id: 10,
                    payload: "second",
                    when: When::Within(first_id),
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
                    payload: "third",
                    when: When::Within(EventId::from(2)),
                }),
            )
            .await
            .expect("record should succeed");

        let page = store
            .review(TestId::Alpha(10), After::Specific(first_id), 1)
            .await
            .expect("review should succeed");

        assert_eq!(page.items.len(), 1);
        assert!(page.more);
        assert!(matches!(&page.items[0], TestEvent::Alpha(event) if event.payload == "second"));
    }

    #[tokio::test]
    async fn observe_returns_global_events_and_respects_after_and_limit() {
        let store = MemoryStore::<TestEvent>::new();
        let Outcome::Recorded(events) = store
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
            .expect("commit should succeed")
        else {
            panic!("expected recorded");
        };

        let page = store
            .observe(After::Specific(events[0].event_id()), 1)
            .await
            .expect("observe should succeed");

        assert_eq!(page.items.len(), 1);
        assert!(!page.more);
        assert!(matches!(&page.items[0], TestEvent::Beta(event) if event.payload == "beta"));
    }

    #[tokio::test]
    async fn commit_records_multiple_events_atomically_in_order() {
        let store = MemoryStore::<TestEvent>::new();
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

        let Outcome::Recorded(events) = result else {
            panic!("expected recorded");
        };
        assert_eq!(events.len(), 2);
        assert_eq!(*events[0].event_id(), 1);
        assert_eq!(*events[1].event_id(), 2);
        assert!(matches!(&events[0], TestEvent::Alpha(event) if event.payload == "alpha"));
        assert!(matches!(&events[1], TestEvent::Beta(event) if event.payload == "beta"));
    }

    #[tokio::test]
    async fn commit_skips_all_records_if_any_condition_fails() {
        let store = MemoryStore::<TestEvent>::new();
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

        let result = store
            .commit(
                1,
                Utc::now(),
                vec![
                    TestRecord::Alpha(Record {
                        id: 10,
                        payload: "conflict",
                        when: When::Empty,
                    }),
                    TestRecord::Beta(Record {
                        id: 20,
                        payload: "should-not-record",
                        when: When::Empty,
                    }),
                ],
            )
            .await
            .expect("commit should succeed");
        let beta_page = store
            .review(TestId::Beta(20), After::Start, 10)
            .await
            .expect("review should succeed");

        assert!(matches!(result, Outcome::Skipped));
        assert!(beta_page.items.is_empty());
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
