use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::error::Error;
use std::ops::Deref;

pub trait Stream {
    type Id: Send + Sync + Copy + Clone;
    type Payload: Send + Sync + Clone;
}

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

/// A condition for recording an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When<T: Copy> {
    /// Record only if the stream is empty.
    Empty,
    /// Record only if the stream has no events beyond `T`.
    Within(T),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After<T: Copy> {
    Start,
    Specific(T),
}

/// A paginated list of items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub more: bool,
    pub next: EventId,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Event<A: Clone + Sized, I: Copy + Clone + Sized, P: Clone + Sized> {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub authority: A,
    pub id: I,
    pub payload: P,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<A: Clone + Sized, I: Copy + Clone + Sized, P: Clone + Sized> {
    Recorded(Event<A, I, P>),
    Skipped,
}

pub trait Store<A, S: Stream>: Send + Sync
where
    A: Send + Sync + Clone,
{
    type Error: Error + Send + Sync + 'static;

    async fn record(
        &self,
        by: A,
        at: DateTime<Utc>,
        id: S::Id,
        payload: S::Payload,
        when: When<EventId>,
    ) -> Result<Outcome<A, S::Id, S::Payload>, Self::Error>;

    async fn review(
        &self,
        id: S::Id,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<Event<A, S::Id, S::Payload>>, Self::Error>;
}

pub trait Observe: Send + Sync {
    type Event: Send + Sync + Clone;
    type Error: Error + Send + Sync + 'static;

    async fn observe(
        &self,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<Self::Event>, Self::Error>;
}

/// Generate the shared test suite for `Store<S>` and `Observe`.
/// The store must use `S::Id = u32` and `S::Payload = String`.
///
/// ```ignore
/// multi_store_tests!(
///     TestStream,
///     Event<i32, u32, String>,
///     TestStore::new()
/// );
/// ```
#[cfg(test)]
macro_rules! multi_store_tests {
    ($stream:ty, $event:ty, $make_store:expr) => {
        use crate::store::After;
        use crate::store::Observe;
        use crate::store::Outcome;
        use crate::store::Store;
        use crate::store::When;
        use chrono::Utc;

        fn make_store() -> impl Store<i32, $stream> + Observe<Event = $event> {
            $make_store
        }

        #[tokio::test]
        async fn record_empty_no_prior() {
            let store = make_store();
            let result = store
                .record(0, Utc::now(), 1u32, "first".to_string(), When::Empty)
                .await
                .expect("should succeed");
            assert!(matches!(result, Outcome::Recorded(_)));
        }

        #[tokio::test]
        async fn record_empty_with_prior() {
            let store = make_store();
            store
                .record(0, Utc::now(), 1u32, "first".to_string(), When::Empty)
                .await
                .expect("should succeed");
            let result = store
                .record(1, Utc::now(), 1u32, "second".to_string(), When::Empty)
                .await
                .expect("should succeed");
            assert!(matches!(result, Outcome::Skipped));
        }

        #[tokio::test]
        async fn record_empty_for_one_id_unaffected_by_other_id() {
            let store = make_store();
            store
                .record(0, Utc::now(), 1u32, "a1".to_string(), When::Empty)
                .await
                .expect("should succeed");
            let result = store
                .record(1, Utc::now(), 2u32, "b1".to_string(), When::Empty)
                .await
                .expect("should succeed");
            assert!(matches!(result, Outcome::Recorded(_)));
        }

        #[tokio::test]
        async fn record_within_eq_last_is_recorded() {
            let store = make_store();
            let Outcome::Recorded(event) = store
                .record(0, Utc::now(), 1u32, "first".to_string(), When::Empty)
                .await
                .expect("should succeed")
            else {
                panic!("expected Recorded");
            };
            let result = store
                .record(
                    1,
                    Utc::now(),
                    1u32,
                    "second".to_string(),
                    When::Within(event.event_id),
                )
                .await
                .expect("should succeed");
            assert!(matches!(result, Outcome::Recorded(_)));
        }

        #[tokio::test]
        async fn record_within_lt_last_is_skipped() {
            let store = make_store();
            let Outcome::Recorded(first_event) = store
                .record(0, Utc::now(), 1u32, "first".to_string(), When::Empty)
                .await
                .expect("should succeed")
            else {
                panic!("expected Recorded");
            };
            store
                .record(
                    1,
                    Utc::now(),
                    1u32,
                    "second".to_string(),
                    When::Within(first_event.event_id),
                )
                .await
                .expect("should succeed");
            let result = store
                .record(
                    2,
                    Utc::now(),
                    1u32,
                    "third".to_string(),
                    When::Within(first_event.event_id),
                )
                .await
                .expect("should succeed");
            assert!(matches!(result, Outcome::Skipped));
        }

        #[tokio::test]
        async fn record_returns_expected_fields() {
            let store = make_store();
            let at = Utc::now();
            let by = 0;
            let Outcome::Recorded(event) = store
                .record(by.clone(), at, 42u32, "payload".to_string(), When::Empty)
                .await
                .expect("should succeed")
            else {
                panic!("expected Recorded");
            };
            assert_eq!(event.id, 42u32);
            assert_eq!(event.payload, "payload");
            assert_eq!(event.timestamp, at);
            assert_eq!(event.authority, by);
        }

        #[tokio::test]
        async fn record_preserves_authority_first_value() {
            let store = make_store();
            let by = 1;
            let Outcome::Recorded(event) = store
                .record(by.clone(), Utc::now(), 1u32, "x".to_string(), When::Empty)
                .await
                .expect("should succeed")
            else {
                panic!("expected Recorded");
            };
            assert_eq!(event.authority, by);
        }

        #[tokio::test]
        async fn record_preserves_authority_second_value() {
            let store = make_store();
            let by = 2;
            let Outcome::Recorded(event) = store
                .record(by.clone(), Utc::now(), 1u32, "x".to_string(), When::Empty)
                .await
                .expect("should succeed")
            else {
                panic!("expected Recorded");
            };
            assert_eq!(event.authority, by);
        }

        #[tokio::test]
        async fn review_empty_returns_no_items() {
            let store = make_store();
            let page = store
                .review(1u32, After::Start, 10)
                .await
                .expect("should succeed");
            assert!(page.items.is_empty());
            assert!(!page.more);
        }

        #[tokio::test]
        async fn review_returns_events_for_id() {
            let store = make_store();
            store
                .record(0, Utc::now(), 1u32, "first".to_string(), When::Empty)
                .await
                .expect("should succeed");
            let page = store
                .review(1u32, After::Start, 10)
                .await
                .expect("should succeed");
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].payload, "first");
        }

        #[tokio::test]
        async fn review_obeys_limit() {
            let store = make_store();
            let Outcome::Recorded(first) = store
                .record(0, Utc::now(), 1u32, "first".to_string(), When::Empty)
                .await
                .expect("should succeed")
            else {
                panic!("expected Recorded");
            };
            store
                .record(
                    1,
                    Utc::now(),
                    1u32,
                    "second".to_string(),
                    When::Within(first.event_id),
                )
                .await
                .expect("should succeed");
            let page = store
                .review(1u32, After::Start, 1)
                .await
                .expect("should succeed");
            assert_eq!(page.items.len(), 1);
            assert!(page.more);
        }

        #[tokio::test]
        async fn observe_empty_returns_no_items() {
            let store = make_store();
            let page = store
                .observe(After::Start, 10)
                .await
                .expect("should succeed");
            assert!(page.items.is_empty());
            assert!(!page.more);
        }

        #[tokio::test]
        async fn observe_respects_limit() {
            let store = make_store();
            let Outcome::Recorded(first) = store
                .record(0, Utc::now(), 1u32, "first".to_string(), When::Empty)
                .await
                .expect("should succeed")
            else {
                panic!("expected Recorded");
            };
            store
                .record(1, Utc::now(), 2u32, "second".to_string(), When::Empty)
                .await
                .expect("should succeed");
            let page = store
                .observe(After::Specific(first.event_id), 1)
                .await
                .expect("should succeed");
            assert_eq!(page.items.len(), 1);
            assert!(!page.more);
        }
    };
}

pub mod memory;
pub mod revised;
#[expect(dead_code)]
pub mod universal;
