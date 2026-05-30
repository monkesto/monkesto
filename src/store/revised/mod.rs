use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::error::Error;
use std::hash::Hash;
use std::ops::Deref;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventId(u64);

impl EventId {
    pub fn next(&self) -> Self {
        EventId(self.0 + 1)
    }
}

impl Deref for EventId {
    type Target = u64;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<u64> for EventId {
    fn from(value: u64) -> Self {
        EventId(value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When<T: Copy> {
    Empty,
    Within(T),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After<T: Copy> {
    Start,
    #[cfg_attr(not(test), expect(dead_code))]
    Specific(T),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub more: bool,
    pub next: EventId,
}

pub trait Stream {
    type Id: Send + Sync + Copy + Clone;
    type Payload: Send + Sync + Clone;
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Event<A: Clone + Sized, S: Stream>
where
    S::Id: Sized,
    S::Payload: Sized,
{
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub authority: A,
    pub id: S::Id,
    pub payload: S::Payload,
}

impl<A, S> Clone for Event<A, S>
where
    A: Clone + Sized,
    S: Stream,
    S::Id: Sized,
    S::Payload: Sized,
{
    fn clone(&self) -> Self {
        Self {
            event_id: self.event_id,
            timestamp: self.timestamp,
            authority: self.authority.clone(),
            id: self.id,
            payload: self.payload.clone(),
        }
    }
}

pub trait RecordFor<E: EventFamily>: Send + Sync + Clone {
    fn id(&self) -> E::Id;
    fn when(&self) -> When<EventId>;
    fn into_event(self, event_id: EventId, authority: E::Authority, timestamp: DateTime<Utc>) -> E;
}

pub trait EventFamily: Send + Sync + Clone + Sized {
    type Id: Send + Sync + Copy + Clone + Eq + Hash;
    type Record: RecordFor<Self>;
    type Authority: Send + Sync + Clone;

    fn event_id(&self) -> EventId;
    fn id(&self) -> Self::Id;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<E> {
    Recorded(Vec<E>),
    Skipped,
}

pub struct Record<S: Stream> {
    pub id: S::Id,
    pub payload: S::Payload,
    pub when: When<EventId>,
}

impl<S> Clone for Record<S>
where
    S: Stream,
{
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            payload: self.payload.clone(),
            when: self.when,
        }
    }
}

pub trait Store<E: EventFamily>: Send + Sync {
    type Error: Error + Send + Sync + 'static;

    async fn record(
        &self,
        by: E::Authority,
        at: DateTime<Utc>,
        record: E::Record,
    ) -> Result<Outcome<E>, Self::Error>;

    async fn commit(
        &self,
        by: E::Authority,
        at: DateTime<Utc>,
        records: Vec<E::Record>,
    ) -> Result<Outcome<E>, Self::Error>;

    async fn review(
        &self,
        id: E::Id,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<E>, Self::Error>;

    #[rustfmt::skip]
    #[cfg_attr(not(test), expect(dead_code))]
    async fn observe(
        &self,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<E>, Self::Error>;
}

#[cfg(test)]
macro_rules! revised_store_contract_tests {
    ($make_store:expr) => {
        mod revised_store_contract {
            use super::*;
            use crate::store::revised::After;
            use crate::store::revised::Event;
            use crate::store::revised::EventFamily;
            use crate::store::revised::EventId;
            use crate::store::revised::Outcome;
            use crate::store::revised::Record;
            use crate::store::revised::RecordFor;
            use crate::store::revised::Store;
            use crate::store::revised::Stream;
            use crate::store::revised::When;
            use chrono::Utc;

            #[derive(Clone, Copy, Debug)]
            pub struct AlphaStream;

            impl Stream for AlphaStream {
                type Id = u32;
                type Payload = &'static str;
            }

            #[derive(Clone, Copy, Debug)]
            pub struct BetaStream;

            impl Stream for BetaStream {
                type Id = u32;
                type Payload = &'static str;
            }

            #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
            pub enum TestId {
                Alpha(u32),
                Beta(u32),
            }

            #[derive(Clone)]
            pub enum TestRecord {
                Alpha(Record<AlphaStream>),
                Beta(Record<BetaStream>),
            }

            #[derive(Clone, Debug)]
            pub enum TestEvent {
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

            #[tokio::test]
            async fn record_empty_no_prior() {
                let store = $make_store;
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
                let store = $make_store;
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
                let store = $make_store;
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
                let store = $make_store;
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
                let store = $make_store;
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
                let store = $make_store;
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
                assert!(
                    matches!(&page.items[0], TestEvent::Alpha(event) if event.payload == "alpha-a")
                );
            }

            #[tokio::test]
            async fn review_respects_after_and_limit() {
                let store = $make_store;
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
                assert!(
                    matches!(&page.items[0], TestEvent::Alpha(event) if event.payload == "second")
                );
            }

            #[tokio::test]
            async fn observe_returns_global_events_and_respects_after_and_limit() {
                let store = $make_store;
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
                assert!(
                    matches!(&page.items[0], TestEvent::Beta(event) if event.payload == "beta")
                );
            }

            #[tokio::test]
            async fn commit_records_multiple_events_atomically_in_order() {
                let store = $make_store;
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
                assert!(
                    matches!(&events[0], TestEvent::Alpha(event) if event.payload == "alpha")
                );
                assert!(matches!(&events[1], TestEvent::Beta(event) if event.payload == "beta"));
            }

            #[tokio::test]
            async fn commit_skips_all_records_if_any_condition_fails() {
                let store = $make_store;
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
        }
    };
}

pub mod memory;
