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
    #[cfg_attr(not(test), allow(dead_code))]
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
    type Payload: Send + Sync + Clone + 'static;
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

pub trait EventFamily: Send + Sync + Clone + Sized {
    type Id: Send + Sync + Copy + Clone + Eq + Hash;
    type Authority: Send + Sync + Clone;

    fn event_id(&self) -> EventId;
    fn id(&self) -> Self::Id;
}

pub trait EventFor<S: Stream>: EventFamily {
    fn id_for(id: S::Id) -> Self::Id;
    fn new_event(
        event_id: EventId,
        authority: Self::Authority,
        timestamp: DateTime<Utc>,
        id: S::Id,
        payload: S::Payload,
    ) -> Self;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<E> {
    Recorded(E),
    Skipped,
}

#[allow(async_fn_in_trait)]
pub trait Store<E: EventFamily>: Send + Sync {
    type Error: Error + Send + Sync + 'static;

    async fn record<S>(
        &self,
        by: E::Authority,
        at: DateTime<Utc>,
        id: S::Id,
        payload: S::Payload,
        when: When<EventId>,
    ) -> Result<Outcome<E>, Self::Error>
    where
        S: Stream,
        E: EventFor<S>;

    async fn review(
        &self,
        id: E::Id,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<E>, Self::Error>;

    #[rustfmt::skip]
    #[cfg_attr(not(test), allow(dead_code))]
    async fn observe(
        &self,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<E>, Self::Error>;
}

#[cfg(test)]
macro_rules! store_contract_tests {
    ($make_store:expr) => {
        mod store_contract {
            use super::*;
            use crate::store::After;
            use crate::store::Event;
            use crate::store::EventFamily;
            use crate::store::EventFor;
            use crate::store::EventId;
            use crate::store::Outcome;
            use crate::store::Store;
            use crate::store::Stream;
            use crate::store::When;
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

            #[derive(Clone, Debug)]
            pub enum TestEvent {
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

            async fn make_store() -> impl Store<TestEvent> {
                $make_store
            }

            #[tokio::test]
            async fn record_empty_no_prior() {
                let store = make_store().await;
                let result = store
                    .record::<AlphaStream>(0, Utc::now(), 1u32, "first", When::Empty)
                    .await
                    .expect("should succeed");
                assert!(matches!(result, Outcome::Recorded(_)));
            }

            #[tokio::test]
            async fn record_empty_with_prior_skips() {
                let store = make_store().await;
                store
                    .record::<AlphaStream>(0, Utc::now(), 1u32, "first", When::Empty)
                    .await
                    .expect("should succeed");
                let result = store
                    .record::<AlphaStream>(1, Utc::now(), 1u32, "second", When::Empty)
                    .await
                    .expect("should succeed");
                assert!(matches!(result, Outcome::Skipped));
            }

            #[tokio::test]
            async fn record_empty_for_one_id_unaffected_by_other_id() {
                let store = make_store().await;
                store
                    .record::<AlphaStream>(0, Utc::now(), 1u32, "a1", When::Empty)
                    .await
                    .expect("should succeed");
                let result = store
                    .record::<AlphaStream>(1, Utc::now(), 2u32, "b1", When::Empty)
                    .await
                    .expect("should succeed");
                assert!(matches!(result, Outcome::Recorded(_)));
            }

            #[tokio::test]
            async fn record_within_last_is_recorded() {
                let store = make_store().await;
                let Outcome::Recorded(events) = store
                    .record::<AlphaStream>(0, Utc::now(), 1u32, "first", When::Empty)
                    .await
                    .expect("should succeed")
                else {
                    panic!("expected Recorded");
                };
                let result = store
                    .record::<AlphaStream>(
                        1,
                        Utc::now(),
                        1u32,
                        "second",
                        When::Within(events.event_id()),
                    )
                    .await
                    .expect("should succeed");
                assert!(matches!(result, Outcome::Recorded(_)));
            }

            #[tokio::test]
            async fn record_within_old_event_id_skips() {
                let store = make_store().await;
                let Outcome::Recorded(events) = store
                    .record::<AlphaStream>(0, Utc::now(), 1u32, "first", When::Empty)
                    .await
                    .expect("should succeed")
                else {
                    panic!("expected Recorded");
                };
                store
                    .record::<AlphaStream>(
                        1,
                        Utc::now(),
                        1u32,
                        "second",
                        When::Within(events.event_id()),
                    )
                    .await
                    .expect("should succeed");
                let result = store
                    .record::<AlphaStream>(
                        2,
                        Utc::now(),
                        1u32,
                        "third",
                        When::Within(events.event_id()),
                    )
                    .await
                    .expect("should succeed");
                assert!(matches!(result, Outcome::Skipped));
            }

            #[tokio::test]
            async fn review_returns_only_requested_stream_id() {
                let store = make_store().await;
                store
                    .record::<AlphaStream>(0, Utc::now(), 1u32, "a1", When::Empty)
                    .await
                    .expect("should succeed");
                store
                    .record::<AlphaStream>(0, Utc::now(), 2u32, "b1", When::Empty)
                    .await
                    .expect("should succeed");
                let page = store
                    .review(TestId::Alpha(1), After::Start, 10)
                    .await
                    .expect("should succeed");
                assert_eq!(page.items.len(), 1);
                assert!(matches!(page.items[0], TestEvent::Alpha(_)));
            }

            #[tokio::test]
            async fn review_respects_after_and_limit() {
                let store = make_store().await;
                let Outcome::Recorded(events) = store
                    .record::<AlphaStream>(0, Utc::now(), 1u32, "first", When::Empty)
                    .await
                    .expect("should succeed")
                else {
                    panic!("expected Recorded");
                };
                store
                    .record::<AlphaStream>(
                        1,
                        Utc::now(),
                        1u32,
                        "second",
                        When::Within(events.event_id()),
                    )
                    .await
                    .expect("should succeed");
                let page = store
                    .review(TestId::Alpha(1), After::Start, 1)
                    .await
                    .expect("should succeed");
                assert_eq!(page.items.len(), 1);
                assert!(page.more);

                let page = store
                    .review(TestId::Alpha(1), After::Specific(events.event_id()), 10)
                    .await
                    .expect("should succeed");
                assert_eq!(page.items.len(), 1);
            }

            #[tokio::test]
            async fn observe_returns_global_events_and_respects_after_and_limit() {
                let store = make_store().await;
                let Outcome::Recorded(events) = store
                    .record::<AlphaStream>(0, Utc::now(), 1u32, "a1", When::Empty)
                    .await
                    .expect("should succeed")
                else {
                    panic!("expected Recorded");
                };
                store
                    .record::<BetaStream>(1, Utc::now(), 2u32, "b1", When::Empty)
                    .await
                    .expect("should succeed");
                let page = store
                    .observe(After::Specific(events.event_id()), 1)
                    .await
                    .expect("should succeed");
                assert_eq!(page.items.len(), 1);
                assert!(!page.more);
                assert!(matches!(page.items[0], TestEvent::Beta(_)));
            }
        }
    };
}

pub mod memory;
pub mod sqlite;
