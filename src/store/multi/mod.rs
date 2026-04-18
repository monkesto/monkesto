use crate::store::After;
use crate::store::EventId;
use crate::store::Stream;
use crate::store::When;
use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::error::Error;

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
#[cfg_attr(not(test), expect(dead_code))]
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
        use crate::store::When;
        use crate::store::multi::Observe;
        use crate::store::multi::Outcome;
        use crate::store::multi::Store;
        use chrono::Utc;

        fn make_store() -> impl Store<i32, $stream> + Observe<Event = $event> {
            $make_store
        }

        // ---------------------------------------------------------------
        // record: When variant × resource state
        // ---------------------------------------------------------------

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

        // ---------------------------------------------------------------
        // review: per-id queries
        // ---------------------------------------------------------------

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
        async fn review_does_not_include_other_ids() {
            let store = make_store();
            store
                .record(0, Utc::now(), 1u32, "a".to_string(), When::Empty)
                .await
                .expect("should succeed");
            store
                .record(1, Utc::now(), 2u32, "b".to_string(), When::Empty)
                .await
                .expect("should succeed");
            let page = store
                .review(1u32, After::Start, 10)
                .await
                .expect("should succeed");
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].payload, "a");
        }

        #[tokio::test]
        async fn review_after_specific_excludes_that_event() {
            let store = make_store();
            let Outcome::Recorded(first) = store
                .record(2, Utc::now(), 1u32, "first".to_string(), When::Empty)
                .await
                .expect("should succeed")
            else {
                panic!("expected Recorded");
            };
            store
                .record(
                    3,
                    Utc::now(),
                    1u32,
                    "second".to_string(),
                    When::Within(first.event_id),
                )
                .await
                .expect("should succeed");
            let page = store
                .review(1u32, After::Specific(first.event_id), 10)
                .await
                .expect("should succeed");
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].payload, "second");
        }

        #[tokio::test]
        async fn review_respects_limit() {
            let store = make_store();
            let mut latest = None;
            for i in 0..5 {
                let when = match latest {
                    Some(event_id) => When::Within(event_id),
                    None => When::Empty,
                };
                let Outcome::Recorded(event) = store
                    .record(i, Utc::now(), 1u32, format!("event {i}"), when)
                    .await
                    .expect("should succeed")
                else {
                    panic!("expected Recorded");
                };
                latest = Some(event.event_id);
            }
            let page = store
                .review(1u32, After::Start, 3)
                .await
                .expect("should succeed");
            assert_eq!(page.items.len(), 3);
            assert!(page.more);
        }

        #[tokio::test]
        async fn review_pagination_walks_all_events() {
            let store = make_store();
            let mut latest = None;
            for i in 0..7 {
                let when = match latest {
                    Some(event_id) => When::Within(event_id),
                    None => When::Empty,
                };
                let Outcome::Recorded(event) = store
                    .record(i, Utc::now(), 1u32, format!("event {i}"), when)
                    .await
                    .expect("should succeed")
                else {
                    panic!("expected Recorded");
                };
                latest = Some(event.event_id);
            }
            let mut all_payloads = Vec::new();
            let mut after = After::Start;
            loop {
                let page = store.review(1u32, after, 3).await.expect("should succeed");
                for item in &page.items {
                    all_payloads.push(item.payload.clone());
                }
                if !page.more {
                    break;
                }
                after = After::Specific(page.next);
            }
            let expected: Vec<String> = (0..7).map(|i| format!("event {i}")).collect();
            assert_eq!(all_payloads, expected);
        }

        // ---------------------------------------------------------------
        // review_all: global stream
        // ---------------------------------------------------------------

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
        async fn observe_returns_events_in_insertion_order() {
            let store = make_store();
            store
                .record(0, Utc::now(), 1u32, "a".to_string(), When::Empty)
                .await
                .expect("should succeed");
            store
                .record(1, Utc::now(), 2u32, "b".to_string(), When::Empty)
                .await
                .expect("should succeed");
            let page = store
                .observe(After::Start, 10)
                .await
                .expect("should succeed");
            assert_eq!(page.items.len(), 2);
        }

        #[tokio::test]
        async fn observe_respects_limit() {
            let store = make_store();
            for i in 0..5u32 {
                store
                    .record(i as i32, Utc::now(), i, format!("event {i}"), When::Empty)
                    .await
                    .expect("should succeed");
            }
            let page = store
                .observe(After::Start, 3)
                .await
                .expect("should succeed");
            assert_eq!(page.items.len(), 3);
            assert!(page.more);
        }

        #[tokio::test]
        async fn observe_after_specific_excludes_earlier() {
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
                .observe(After::Specific(first.event_id), 10)
                .await
                .expect("should succeed");
            assert_eq!(page.items.len(), 1);
        }

        #[tokio::test]
        async fn observe_pagination_walks_all_events() {
            let store = make_store();
            for i in 0..7u32 {
                store
                    .record(i as i32, Utc::now(), i, format!("event {i}"), When::Empty)
                    .await
                    .expect("should succeed");
            }
            let mut count = 0;
            let mut after = After::Start;
            loop {
                let page = store.observe(after, 3).await.expect("should succeed");
                count += page.items.len();
                if !page.more {
                    break;
                }
                after = After::Specific(page.next);
            }
            assert_eq!(count, 7);
        }

        #[tokio::test]
        async fn observe_skipped_record_does_not_appear() {
            let store = make_store();
            store
                .record(0, Utc::now(), 1u32, "first".to_string(), When::Empty)
                .await
                .expect("should succeed");
            // This should be skipped — id 1 already has events
            store
                .record(
                    1,
                    Utc::now(),
                    1u32,
                    "should not appear".to_string(),
                    When::Empty,
                )
                .await
                .expect("should succeed");
            let page = store
                .observe(After::Start, 10)
                .await
                .expect("should succeed");
            assert_eq!(page.items.len(), 1);
        }

        #[tokio::test]
        async fn event_ids_are_globally_ordered() {
            let store = make_store();
            store
                .record(0, Utc::now(), 1u32, "a".to_string(), When::Empty)
                .await
                .expect("should succeed");
            store
                .record(1, Utc::now(), 2u32, "b".to_string(), When::Empty)
                .await
                .expect("should succeed");

            let id1 = store
                .review(1u32, After::Start, 1)
                .await
                .expect("should succeed")
                .items[0]
                .event_id;
            let id2 = store
                .review(2u32, After::Start, 1)
                .await
                .expect("should succeed")
                .items[0]
                .event_id;
            assert!(id1 < id2);
        }
    };
}

pub mod memory;
