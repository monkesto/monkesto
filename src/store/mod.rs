use crate::authority::Authority;
use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use std::error::Error;
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

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct Event<I: Copy + Clone + Sized, P: Clone + Sized> {
    pub event_id: EventId,
    pub timestamp: DateTime<Utc>,
    pub authority: Authority,
    pub id: I,
    pub payload: P,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<I: Copy + Clone + Sized, P: Clone + Sized> {
    Recorded(Event<I, P>),
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Select<T: Copy> {
    #[cfg_attr(not(test), expect(dead_code))]
    All,
    #[cfg_attr(not(test), expect(dead_code))]
    One(T),
}

/// A condition for recording an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When<T: Copy> {
    #[cfg_attr(not(test), expect(dead_code))]
    Always,
    #[cfg_attr(not(test), expect(dead_code))]
    Current(T),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After<T: Copy> {
    #[cfg_attr(not(test), expect(dead_code))]
    Start,
    #[cfg_attr(not(test), expect(dead_code))]
    Specific(T),
}

/// A page of events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Page<I: Copy + Clone + Sized, P: Clone + Sized> {
    pub items: Vec<Event<I, P>>,
    /// Whether there are currently more pages for this query.
    pub more: bool,
    /// The value to use as the `after` param to get the next page for this query.
    pub next: EventId,
}

#[cfg_attr(not(test), expect(dead_code))]
pub trait Store: Send + Sync {
    type Id: Send + Sync + Copy + Clone;
    type Payload: Send + Sync + Clone;
    type Error: Error + Send + Sync + 'static;

    async fn record(
        &self,
        by: Authority,
        at: DateTime<Utc>,
        id: Self::Id,
        payload: Self::Payload,
        when: When<EventId>,
    ) -> Result<Outcome<Self::Id, Self::Payload>, Self::Error>;

    async fn review(
        &self,
        select: Select<Self::Id>,
        after: After<EventId>,
        limit: usize,
    ) -> Result<Page<Self::Id, Self::Payload>, Self::Error>;
}

/// Generates the shared `Store` test suite. Pass an expression that constructs
/// a fresh, empty store. The store must implement `Store<Id = u32, Payload = String>`.
///
/// ```ignore
/// store_tests!(MemoryStore::<u32, String>::default());
/// ```
#[cfg(test)]
macro_rules! store_tests {
    ($make_store:expr) => {
        use crate::auth::user::UserId;
        use crate::authority::Actor;
        use crate::authority::Authority;
        use crate::grant::GrantId;
        use crate::store::After;
        use crate::store::EventId;
        use crate::store::Outcome;
        use crate::store::Select;
        use crate::store::Store;
        use crate::store::When;
        use chrono::Utc;
        use rstest::rstest;

        // Tests assume Id = u32 and Payload = String. This assertion is never
        // executed — it exists so that a type mismatch produces an error near the
        // macro invocation rather than deep inside a test body.
        const _: () = {
            fn check(_: &impl Store<Id = u32, Payload = String>) {}
            #[expect(dead_code)]
            fn assert() {
                check(&$make_store);
            }
        };

        // ---------------------------------------------------------------
        // record: `When` variant × resource state
        // ---------------------------------------------------------------

        #[tokio::test]
        async fn record_always_no_prior() {
            let store = $make_store;
            let result = store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "first".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            assert!(matches!(result, Outcome::Recorded(_)));
        }

        #[tokio::test]
        async fn record_always_with_prior() {
            let store = $make_store;
            store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "first".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            let result = store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "second".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            assert!(matches!(result, Outcome::Recorded(_)));
        }

        #[tokio::test]
        async fn record_current_eq_last_is_recorded() {
            let store = $make_store;
            let first = store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "first".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            let Outcome::Recorded(event) = first else {
                panic!("expected Recorded");
            };
            let result = store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "second".to_string(),
                    When::Current(event.event_id),
                )
                .await
                .unwrap();
            assert!(matches!(result, Outcome::Recorded(_)));
        }

        #[tokio::test]
        async fn record_current_gt_last_is_recorded() {
            let store = $make_store;
            let first = store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "first".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            let Outcome::Recorded(event) = first else {
                panic!("expected Recorded");
            };
            // Use an event_id greater than the resource's last
            let future_id = event.event_id.next();
            let result = store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "second".to_string(),
                    When::Current(future_id),
                )
                .await
                .unwrap();
            assert!(matches!(result, Outcome::Recorded(_)));
        }

        #[tokio::test]
        async fn record_current_lt_last_is_skipped() {
            let store = $make_store;
            let first = store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "first".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            let Outcome::Recorded(first_event) = first else {
                panic!("expected Recorded");
            };
            // Record again to advance the resource's last event_id
            store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "second".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            // Use the first event_id, which is now behind
            let result = store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "third".to_string(),
                    When::Current(first_event.event_id),
                )
                .await
                .unwrap();
            assert!(matches!(result, Outcome::Skipped));
        }

        // ---------------------------------------------------------------
        // record: field preservation and independence
        // ---------------------------------------------------------------

        #[tokio::test]
        async fn record_returns_expected_fields() {
            let store = $make_store;
            let at = Utc::now();
            let by = Authority::Direct(Actor::System);
            let result = store
                .record(by.clone(), at, 42u32, "payload".to_string(), When::Always)
                .await
                .unwrap();
            let Outcome::Recorded(event) = result else {
                panic!("expected Recorded");
            };
            assert_eq!(event.id, 42u32);
            assert_eq!(event.payload, "payload");
            assert_eq!(event.timestamp, at);
            assert_eq!(event.authority, by);
        }

        #[tokio::test]
        async fn record_sequential_event_ids_are_increasing() {
            let store = $make_store;
            let mut prev_id = EventId::from(0);
            for i in 0..5 {
                let result = store
                    .record(
                        Authority::Direct(Actor::System),
                        Utc::now(),
                        1u32,
                        format!("event {i}"),
                        When::Always,
                    )
                    .await
                    .unwrap();
                let Outcome::Recorded(event) = result else {
                    panic!("expected Recorded");
                };
                assert!(event.event_id > prev_id);
                prev_id = event.event_id;
            }
        }

        #[tokio::test]
        async fn record_preserves_authority_direct() {
            let store = $make_store;
            let by = Authority::Direct(Actor::User(UserId::new()));
            let result = store
                .record(by.clone(), Utc::now(), 1u32, "x".to_string(), When::Always)
                .await
                .unwrap();
            let Outcome::Recorded(event) = result else {
                panic!("expected Recorded");
            };
            assert_eq!(event.authority, by);
        }

        #[tokio::test]
        async fn record_preserves_authority_delegated() {
            let store = $make_store;
            let by = Authority::Delegated {
                grantor: Actor::User(UserId::new()),
                grant: GrantId::new(),
                grantee: Actor::User(UserId::new()),
            };
            let result = store
                .record(by.clone(), Utc::now(), 1u32, "x".to_string(), When::Always)
                .await
                .unwrap();
            let Outcome::Recorded(event) = result else {
                panic!("expected Recorded");
            };
            assert_eq!(event.authority, by);
        }

        #[tokio::test]
        async fn record_preserves_timestamp() {
            let store = $make_store;
            let at = chrono::DateTime::parse_from_rfc3339("2025-06-15T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc);
            let result = store
                .record(
                    Authority::Direct(Actor::System),
                    at,
                    1u32,
                    "x".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            let Outcome::Recorded(event) = result else {
                panic!("expected Recorded");
            };
            assert_eq!(event.timestamp, at);
        }

        #[tokio::test]
        async fn record_out_of_order_timestamps_stored_in_insertion_order() {
            let store = $make_store;
            let later = chrono::DateTime::parse_from_rfc3339("2025-06-15T12:00:00Z")
                .unwrap()
                .with_timezone(&Utc);
            let earlier = chrono::DateTime::parse_from_rfc3339("2025-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc);
            // Record the later timestamp first
            store
                .record(
                    Authority::Direct(Actor::System),
                    later,
                    1u32,
                    "later".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            store
                .record(
                    Authority::Direct(Actor::System),
                    earlier,
                    1u32,
                    "earlier".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            let page = store.review(Select::All, After::Start, 10).await.unwrap();
            assert_eq!(page.items.len(), 2);
            assert_eq!(page.items[0].timestamp, later);
            assert_eq!(page.items[1].timestamp, earlier);
        }

        #[tokio::test]
        async fn record_different_ids_have_independent_histories() {
            let store = $make_store;
            store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "a1".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    2u32,
                    "b1".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            let page_a = store
                .review(Select::One(1u32), After::Start, 10)
                .await
                .unwrap();
            let page_b = store
                .review(Select::One(2u32), After::Start, 10)
                .await
                .unwrap();
            assert_eq!(page_a.items.len(), 1);
            assert_eq!(page_a.items[0].payload, "a1");
            assert_eq!(page_b.items.len(), 1);
            assert_eq!(page_b.items[0].payload, "b1");
        }

        #[tokio::test]
        async fn record_current_for_one_id_unaffected_by_other_id() {
            let store = $make_store;
            // Record to id 1 and capture its event_id
            let result = store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "a1".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            let Outcome::Recorded(event_a) = result else {
                panic!("expected Recorded");
            };
            // Record to id 2, advancing the global event_id
            store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    2u32,
                    "b1".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            // Current check on id 1 with its own last event_id should still succeed
            let result = store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "a2".to_string(),
                    When::Current(event_a.event_id),
                )
                .await
                .unwrap();
            assert!(matches!(result, Outcome::Recorded(_)));
        }

        #[tokio::test]
        async fn record_skipped_leaves_store_unchanged() {
            let store = $make_store;
            let result = store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "first".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            let Outcome::Recorded(first_event) = result else {
                panic!("expected Recorded");
            };
            // Advance the resource's event_id
            store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "second".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            let before = store.review(Select::All, After::Start, 10).await.unwrap();
            // Attempt a stale write — should be skipped
            let result = store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "should not appear".to_string(),
                    When::Current(first_event.event_id),
                )
                .await
                .unwrap();
            assert!(matches!(result, Outcome::Skipped));
            let after = store.review(Select::All, After::Start, 10).await.unwrap();
            assert_eq!(before, after);
        }

        // ---------------------------------------------------------------
        // review: `Select` × `After` × limit
        // ---------------------------------------------------------------

        #[rstest]
        #[case::all_start(Select::All, After::Start)]
        #[case::all_specific(Select::All, After::Specific(EventId::from(0)))]
        #[case::one_start(Select::One(1u32), After::Start)]
        #[case::one_specific(Select::One(1u32), After::Specific(EventId::from(0)))]
        #[tokio::test]
        async fn review_empty_results(#[case] select: Select<u32>, #[case] after: After<EventId>) {
            let store = $make_store;
            let page = store.review(select, after, 10).await.unwrap();
            assert!(page.items.is_empty());
            assert!(!page.more);
        }

        #[rstest]
        #[case::all_start(Select::All, After::Start)]
        #[case::all_specific(Select::All, After::Specific(EventId::from(0)))]
        #[case::one_start(Select::One(1u32), After::Start)]
        #[case::one_specific(Select::One(1u32), After::Specific(EventId::from(0)))]
        #[tokio::test]
        async fn review_results_under_limit(
            #[case] select: Select<u32>,
            #[case] after: After<EventId>,
        ) {
            let store = $make_store;
            for i in 0..3 {
                store
                    .record(
                        Authority::Direct(Actor::System),
                        Utc::now(),
                        1u32,
                        format!("event {i}"),
                        When::Always,
                    )
                    .await
                    .unwrap();
            }
            let page = store.review(select, after, 10).await.unwrap();
            assert_eq!(page.items.len(), 3);
            assert!(!page.more);
        }

        #[rstest]
        #[case::all_start(Select::All, After::Start)]
        #[case::all_specific(Select::All, After::Specific(EventId::from(0)))]
        #[case::one_start(Select::One(1u32), After::Start)]
        #[case::one_specific(Select::One(1u32), After::Specific(EventId::from(0)))]
        #[tokio::test]
        async fn review_results_equal_limit(
            #[case] select: Select<u32>,
            #[case] after: After<EventId>,
        ) {
            let store = $make_store;
            for i in 0..3 {
                store
                    .record(
                        Authority::Direct(Actor::System),
                        Utc::now(),
                        1u32,
                        format!("event {i}"),
                        When::Always,
                    )
                    .await
                    .unwrap();
            }
            let page = store.review(select, after, 3).await.unwrap();
            assert_eq!(page.items.len(), 3);
            assert!(!page.more);
        }

        #[rstest]
        #[case::all_start(Select::All, After::Start)]
        #[case::all_specific(Select::All, After::Specific(EventId::from(0)))]
        #[case::one_start(Select::One(1u32), After::Start)]
        #[case::one_specific(Select::One(1u32), After::Specific(EventId::from(0)))]
        #[tokio::test]
        async fn review_results_over_limit(
            #[case] select: Select<u32>,
            #[case] after: After<EventId>,
        ) {
            let store = $make_store;
            for i in 0..5 {
                store
                    .record(
                        Authority::Direct(Actor::System),
                        Utc::now(),
                        1u32,
                        format!("event {i}"),
                        When::Always,
                    )
                    .await
                    .unwrap();
            }
            let page = store.review(select, after, 3).await.unwrap();
            assert_eq!(page.items.len(), 3);
            assert!(page.more);
        }

        // ---------------------------------------------------------------
        // review: ordering and pagination
        // ---------------------------------------------------------------

        #[tokio::test]
        async fn review_returns_insertion_order() {
            let store = $make_store;
            // Record across different ids to ensure global ordering
            for (id, payload) in [(1u32, "a"), (2u32, "b"), (1u32, "c"), (2u32, "d")] {
                store
                    .record(
                        Authority::Direct(Actor::System),
                        Utc::now(),
                        id,
                        payload.to_string(),
                        When::Always,
                    )
                    .await
                    .unwrap();
            }
            let page = store.review(Select::All, After::Start, 10).await.unwrap();
            let payloads: Vec<&str> = page.items.iter().map(|e| e.payload.as_str()).collect();
            assert_eq!(payloads, vec!["a", "b", "c", "d"]);
        }

        #[tokio::test]
        async fn review_after_specific_excludes_that_event() {
            let store = $make_store;
            let result = store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "first".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            let Outcome::Recorded(first) = result else {
                panic!("expected Recorded");
            };
            store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "second".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            let page = store
                .review(Select::All, After::Specific(first.event_id), 10)
                .await
                .unwrap();
            assert_eq!(page.items.len(), 1);
            assert_eq!(page.items[0].payload, "second");
        }

        #[tokio::test]
        async fn review_after_specific_beyond_latest_returns_empty() {
            let store = $make_store;
            store
                .record(
                    Authority::Direct(Actor::System),
                    Utc::now(),
                    1u32,
                    "first".to_string(),
                    When::Always,
                )
                .await
                .unwrap();
            let far_future = EventId::from(9999);
            let page = store
                .review(Select::All, After::Specific(far_future), 10)
                .await
                .unwrap();
            assert!(page.items.is_empty());
            assert!(!page.more);
        }

        #[tokio::test]
        async fn review_page_next_cursor_walks_without_duplicates_or_gaps() {
            let store = $make_store;
            for i in 0..5 {
                store
                    .record(
                        Authority::Direct(Actor::System),
                        Utc::now(),
                        1u32,
                        format!("event {i}"),
                        When::Always,
                    )
                    .await
                    .unwrap();
            }
            // Walk in pages of 2
            let page1 = store.review(Select::All, After::Start, 2).await.unwrap();
            assert_eq!(page1.items.len(), 2);
            assert!(page1.more);

            let page2 = store
                .review(Select::All, After::Specific(page1.next), 2)
                .await
                .unwrap();
            assert_eq!(page2.items.len(), 2);
            assert!(page2.more);

            // No overlap between pages
            assert!(page2.items[0].event_id > page1.items[1].event_id);
            // No gap — page2 starts right after page1
            let page1_payloads: Vec<&str> =
                page1.items.iter().map(|e| e.payload.as_str()).collect();
            let page2_payloads: Vec<&str> =
                page2.items.iter().map(|e| e.payload.as_str()).collect();
            assert_eq!(page1_payloads, vec!["event 0", "event 1"]);
            assert_eq!(page2_payloads, vec!["event 2", "event 3"]);
        }

        #[tokio::test]
        async fn review_walking_all_pages_yields_every_event_once() {
            let store = $make_store;
            for i in 0..7 {
                store
                    .record(
                        Authority::Direct(Actor::System),
                        Utc::now(),
                        1u32,
                        format!("event {i}"),
                        When::Always,
                    )
                    .await
                    .unwrap();
            }
            let mut all_payloads = Vec::new();
            let mut after = After::Start;
            loop {
                let page = store.review(Select::All, after, 3).await.unwrap();
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
    };
}

// Implementation modules must happen *after* the store_tests macro is created,
// so that the macro exists when the modules are being compiled.
// This is textual ordering, a unique quirk of macros.
pub mod memory;
