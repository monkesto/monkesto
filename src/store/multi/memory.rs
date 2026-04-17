#[cfg(test)]
/// A multi-stream in-memory store.
///
/// Implemented as a struct with a shared `EventId` counter and per-stream state,
/// with concrete `Store<S>` impls for each listed stream and optionally
/// an `Observe` impl for the global event stream.
///
/// ```ignore
/// // Single stream — generates Store<S> + Observe with bare Event type
/// memory_store! {
///     type AppStore = MemoryStore<RoleStream>
/// }
///
/// // Multiple streams without Observe
/// memory_store! {
///     type AppStore = MemoryStore<RoleStream, GrantStream>
/// }
///
/// // Multiple streams with Observe — requires an event enum and variant mapping
/// memory_store! {
///     type AppStore = MemoryStore<RoleStream, GrantStream>
///     where MyEvent {
///         RoleStream => Role,
///         GrantStream => Grant,
///     }
/// }
/// ```
macro_rules! memory_store {
    // Single stream — generates Store<S> + Observe with bare Event type
    (type $name:ident = MemoryStore<$single:ident>) => {
        memory_store!(@single_stream $name, $single);
    };
    // Multiple streams with event mapping — generates Store<S> per stream + Observe
    (type $name:ident = MemoryStore<$($stream:ident),+ $(,)?>
     where $event:ident {
         $($estream:ident => $variant:ident),+ $(,)?
     }
    ) => {
        memory_store!(@with_events $name,
            streams: [$($stream),+],
            event: $event { $($estream => $variant),+ });
    };
    // Multiple streams without event mapping — generates Store<S> per stream, no Observe
    (type $name:ident = MemoryStore<$($stream:ident),+ $(,)?>) => {
        memory_store!(@without_events $name, streams: [$($stream),+]);
    };
    // Internal: multiple streams without events
    (@without_events $name:ident, streams: [$($stream:ident),+]) => {
        memory_store!(@detail);

        paste::paste! {
            memory_store!(@struct $name : $($stream),+);

            $(
                memory_store!(@store_impl $name, $stream);
            )+
        }
    };
    // Internal: single stream — generates Store<S> + Observe with bare Event type
    (@single_stream $name:ident, $stream:ident) => {
        memory_store!(@detail);

        paste::paste! {
            struct [<$name State>] {
                latest: crate::store::EventId,
                [<$stream:snake>]: __memory_store_detail::StreamState<
                    <$stream as crate::store::Stream>::Id,
                    <$stream as crate::store::Stream>::Payload,
                >,
                global_events: Vec<crate::store::Event<
                    <$stream as crate::store::Stream>::Id,
                    <$stream as crate::store::Stream>::Payload,
                >>,
            }

            #[derive(Clone)]
            pub struct $name {
                state: std::sync::Arc<std::sync::Mutex<[<$name State>]>>,
            }

            impl $name {
                pub fn new() -> Self {
                    Self {
                        state: std::sync::Arc::new(std::sync::Mutex::new([<$name State>] {
                            latest: crate::store::EventId::from(0),
                            [<$stream:snake>]: __memory_store_detail::StreamState::default(),
                            global_events: Vec::new(),
                        })),
                    }
                }
            }

            impl crate::store::multi::Store<$stream> for $name {
                type Error = std::convert::Infallible;

                async fn record(
                    &self,
                    by: crate::authority::Authority,
                    at: chrono::DateTime<chrono::Utc>,
                    id: <$stream as crate::store::Stream>::Id,
                    payload: <$stream as crate::store::Stream>::Payload,
                    when: crate::store::When<crate::store::EventId>,
                ) -> Result<
                    crate::store::Outcome<
                        <$stream as crate::store::Stream>::Id,
                        <$stream as crate::store::Stream>::Payload,
                    >,
                    Self::Error,
                > {
                    let mut state = self.state.lock().expect("poisoned");
                    let stream = &mut state.[<$stream:snake>];
                    match when {
                        crate::store::When::Empty => {
                            if stream.select_events.contains_key(&id) {
                                return Ok(crate::store::Outcome::Skipped);
                            }
                        }
                        crate::store::When::Within(event_id) => {
                            let condition_met = stream
                                .select_events
                                .get(&id)
                                .and_then(|v| v.last())
                                .map(|e| e.event_id <= event_id)
                                .unwrap_or(false);
                            if !condition_met {
                                return Ok(crate::store::Outcome::Skipped);
                            }
                        }
                    }
                    state.latest = state.latest.next();
                    let event = crate::store::Event {
                        event_id: state.latest,
                        timestamp: at,
                        authority: by,
                        id,
                        payload,
                    };
                    let stream = &mut state.[<$stream:snake>];
                    stream.events.push(event.clone());
                    stream
                        .select_events
                        .entry(id)
                        .or_default()
                        .push(event.clone());
                    state.global_events.push(event.clone());
                    Ok(crate::store::Outcome::Recorded(event))
                }

                async fn review(
                    &self,
                    id: <$stream as crate::store::Stream>::Id,
                    after: crate::store::After<crate::store::EventId>,
                    limit: usize,
                ) -> Result<
                    crate::store::multi::Page<crate::store::Event<
                        <$stream as crate::store::Stream>::Id,
                        <$stream as crate::store::Stream>::Payload,
                    >>,
                    Self::Error,
                > {
                    let state = self.state.lock().expect("poisoned");
                    let stream = &state.[<$stream:snake>];
                    let full = stream.select_events.get(&id).cloned().unwrap_or_default();
                    let filtered: Vec<crate::store::Event<
                        <$stream as crate::store::Stream>::Id,
                        <$stream as crate::store::Stream>::Payload,
                    >> = match after {
                        crate::store::After::Start => {
                            full.into_iter().take(limit + 1).collect()
                        }
                        crate::store::After::Specific(event_id) => full
                            .into_iter()
                            .filter(|e| e.event_id > event_id)
                            .take(limit + 1)
                            .collect(),
                    };
                    let items: Vec<_> =
                        filtered.clone().into_iter().take(limit).collect();
                    let more = filtered.len() > limit;
                    let next = items
                        .last()
                        .map(|e| e.event_id)
                        .unwrap_or(state.latest);
                    Ok(crate::store::multi::Page { items, more, next })
                }
            }

            impl crate::store::multi::Observe for $name {
                type Event = crate::store::Event<
                    <$stream as crate::store::Stream>::Id,
                    <$stream as crate::store::Stream>::Payload,
                >;
                type Error = std::convert::Infallible;

                async fn observe(
                    &self,
                    after: crate::store::After<crate::store::EventId>,
                    limit: usize,
                ) -> Result<crate::store::multi::Page<Self::Event>, Self::Error> {
                    let state = self.state.lock().expect("poisoned");
                    let full = &state.global_events;
                    let filtered: Vec<Self::Event> = match after {
                        crate::store::After::Start => {
                            full.iter().take(limit + 1).cloned().collect()
                        }
                        crate::store::After::Specific(event_id) => full
                            .iter()
                            .filter(|e| e.event_id > event_id)
                            .take(limit + 1)
                            .cloned()
                            .collect(),
                    };
                    let items: Vec<_> =
                        filtered.clone().into_iter().take(limit).collect();
                    let more = filtered.len() > limit;
                    let next = items
                        .last()
                        .map(|e| e.event_id)
                        .unwrap_or(state.latest);
                    Ok(crate::store::multi::Page { items, more, next })
                }
            }
        }
    };
    // Internal: with events
    (@with_events $name:ident,
     streams: [$($stream:ident),+],
     event: $event:ident { $($estream:ident => $variant:ident),+ }
    ) => {
        memory_store!(@detail);

        paste::paste! {
            memory_store!(@struct $name : $($stream),+; event: $event);

            $(
                memory_store!(@store_impl_with_event $name, $estream, $event, $variant);
            )+

            impl crate::store::multi::Observe for $name {
                type Event = $event;
                type Error = std::convert::Infallible;

                async fn observe(
                    &self,
                    after: crate::store::After<crate::store::EventId>,
                    limit: usize,
                ) -> Result<crate::store::multi::Page<Self::Event>, Self::Error> {
                    let state = self.state.lock().expect("poisoned");
                    let full = &state.global_events;
                    let filtered: Vec<$event> = match after {
                        crate::store::After::Start => {
                            full.iter().take(limit + 1).cloned().collect()
                        }
                        crate::store::After::Specific(event_id) => full
                            .iter()
                            .filter(|e| {
                                let eid = match e {
                                    $($event::$variant(inner) => inner.event_id,)+
                                };
                                eid > event_id
                            })
                            .take(limit + 1)
                            .cloned()
                            .collect(),
                    };
                    let items: Vec<_> =
                        filtered.clone().into_iter().take(limit).collect();
                    let more = filtered.len() > limit;
                    let next = items
                        .last()
                        .map(|e| match e {
                            $($event::$variant(inner) => inner.event_id,)+
                        })
                        .unwrap_or(state.latest);
                    Ok(crate::store::multi::Page { items, more, next })
                }
            }
        }
    };
    // Helper: StreamState detail module
    (@detail) => {
        mod __memory_store_detail {
            use crate::store::Event;
            use std::collections::HashMap;
            use std::hash::Hash;

            pub struct StreamState<I, P>
            where
                I: Send + Sync + Copy + Clone + Eq + Hash + Sized,
                P: Send + Sync + Clone,
            {
                pub events: Vec<Event<I, P>>,
                pub select_events: HashMap<I, Vec<Event<I, P>>>,
            }

            impl<I, P> Default for StreamState<I, P>
            where
                I: Send + Sync + Copy + Clone + Eq + Hash + Sized,
                P: Send + Sync + Clone,
            {
                fn default() -> Self {
                    Self {
                        events: Vec::new(),
                        select_events: HashMap::new(),
                    }
                }
            }
        }
    };
    // Helper: struct without global_events
    (@struct $name:ident : $($stream:ident),+) => {
        paste::paste! {
            struct [<$name State>] {
                latest: crate::store::EventId,
                $(
                    [<$stream:snake>]: __memory_store_detail::StreamState<
                        <$stream as crate::store::Stream>::Id,
                        <$stream as crate::store::Stream>::Payload,
                    >,
                )+
            }

            #[derive(Clone)]
            pub struct $name {
                state: std::sync::Arc<std::sync::Mutex<[<$name State>]>>,
            }

            impl $name {
                pub fn new() -> Self {
                    Self {
                        state: std::sync::Arc::new(std::sync::Mutex::new([<$name State>] {
                            latest: crate::store::EventId::from(0),
                            $(
                                [<$stream:snake>]: __memory_store_detail::StreamState::default(),
                            )+
                        })),
                    }
                }
            }
        }
    };
    // Helper: struct with global_events
    (@struct $name:ident : $($stream:ident),+; event: $event:ident) => {
        paste::paste! {
            struct [<$name State>] {
                latest: crate::store::EventId,
                $(
                    [<$stream:snake>]: __memory_store_detail::StreamState<
                        <$stream as crate::store::Stream>::Id,
                        <$stream as crate::store::Stream>::Payload,
                    >,
                )+
                global_events: Vec<$event>,
            }

            #[derive(Clone)]
            pub struct $name {
                state: std::sync::Arc<std::sync::Mutex<[<$name State>]>>,
            }

            impl $name {
                pub fn new() -> Self {
                    Self {
                        state: std::sync::Arc::new(std::sync::Mutex::new([<$name State>] {
                            latest: crate::store::EventId::from(0),
                            $(
                                [<$stream:snake>]: __memory_store_detail::StreamState::default(),
                            )+
                            global_events: Vec::new(),
                        })),
                    }
                }
            }
        }
    };
    // Helper: Store<S> impl without global push
    (@store_impl $name:ident, $stream:ident) => {
        paste::paste! {
            impl crate::store::multi::Store<$stream> for $name {
                type Error = std::convert::Infallible;

                async fn record(
                    &self,
                    by: crate::authority::Authority,
                    at: chrono::DateTime<chrono::Utc>,
                    id: <$stream as crate::store::Stream>::Id,
                    payload: <$stream as crate::store::Stream>::Payload,
                    when: crate::store::When<crate::store::EventId>,
                ) -> Result<
                    crate::store::Outcome<
                        <$stream as crate::store::Stream>::Id,
                        <$stream as crate::store::Stream>::Payload,
                    >,
                    Self::Error,
                > {
                    let mut state = self.state.lock().expect("poisoned");
                    let stream = &mut state.[<$stream:snake>];
                    match when {
                        crate::store::When::Empty => {
                            if stream.select_events.contains_key(&id) {
                                return Ok(crate::store::Outcome::Skipped);
                            }
                        }
                        crate::store::When::Within(event_id) => {
                            let condition_met = stream
                                .select_events
                                .get(&id)
                                .and_then(|v| v.last())
                                .map(|e| e.event_id <= event_id)
                                .unwrap_or(false);
                            if !condition_met {
                                return Ok(crate::store::Outcome::Skipped);
                            }
                        }
                    }
                    state.latest = state.latest.next();
                    let event = crate::store::Event {
                        event_id: state.latest,
                        timestamp: at,
                        authority: by,
                        id,
                        payload,
                    };
                    let stream = &mut state.[<$stream:snake>];
                    stream.events.push(event.clone());
                    stream
                        .select_events
                        .entry(id)
                        .or_default()
                        .push(event.clone());
                    Ok(crate::store::Outcome::Recorded(event))
                }

                async fn review(
                    &self,
                    id: <$stream as crate::store::Stream>::Id,
                    after: crate::store::After<crate::store::EventId>,
                    limit: usize,
                ) -> Result<
                    crate::store::multi::Page<crate::store::Event<
                        <$stream as crate::store::Stream>::Id,
                        <$stream as crate::store::Stream>::Payload,
                    >>,
                    Self::Error,
                > {
                    let state = self.state.lock().expect("poisoned");
                    let stream = &state.[<$stream:snake>];
                    let full = stream.select_events.get(&id).cloned().unwrap_or_default();
                    let filtered: Vec<crate::store::Event<
                        <$stream as crate::store::Stream>::Id,
                        <$stream as crate::store::Stream>::Payload,
                    >> = match after {
                        crate::store::After::Start => {
                            full.into_iter().take(limit + 1).collect()
                        }
                        crate::store::After::Specific(event_id) => full
                            .into_iter()
                            .filter(|e| e.event_id > event_id)
                            .take(limit + 1)
                            .collect(),
                    };
                    let items: Vec<_> =
                        filtered.clone().into_iter().take(limit).collect();
                    let more = filtered.len() > limit;
                    let next = items
                        .last()
                        .map(|e| e.event_id)
                        .unwrap_or(state.latest);
                    Ok(crate::store::multi::Page { items, more, next })
                }
            }
        }
    };
    // Helper: Store<S> impl with global push
    (@store_impl_with_event $name:ident, $stream:ident, $event:ident, $variant:ident) => {
        paste::paste! {
            impl crate::store::multi::Store<$stream> for $name {
                type Error = std::convert::Infallible;

                async fn record(
                    &self,
                    by: crate::authority::Authority,
                    at: chrono::DateTime<chrono::Utc>,
                    id: <$stream as crate::store::Stream>::Id,
                    payload: <$stream as crate::store::Stream>::Payload,
                    when: crate::store::When<crate::store::EventId>,
                ) -> Result<
                    crate::store::Outcome<
                        <$stream as crate::store::Stream>::Id,
                        <$stream as crate::store::Stream>::Payload,
                    >,
                    Self::Error,
                > {
                    let mut state = self.state.lock().expect("poisoned");
                    let stream = &mut state.[<$stream:snake>];
                    match when {
                        crate::store::When::Empty => {
                            if stream.select_events.contains_key(&id) {
                                return Ok(crate::store::Outcome::Skipped);
                            }
                        }
                        crate::store::When::Within(event_id) => {
                            let condition_met = stream
                                .select_events
                                .get(&id)
                                .and_then(|v| v.last())
                                .map(|e| e.event_id <= event_id)
                                .unwrap_or(false);
                            if !condition_met {
                                return Ok(crate::store::Outcome::Skipped);
                            }
                        }
                    }
                    state.latest = state.latest.next();
                    let event = crate::store::Event {
                        event_id: state.latest,
                        timestamp: at,
                        authority: by,
                        id,
                        payload,
                    };
                    let stream = &mut state.[<$stream:snake>];
                    stream.events.push(event.clone());
                    stream
                        .select_events
                        .entry(id)
                        .or_default()
                        .push(event.clone());
                    state.global_events.push($event::$variant(event.clone()));
                    Ok(crate::store::Outcome::Recorded(event))
                }

                async fn review(
                    &self,
                    id: <$stream as crate::store::Stream>::Id,
                    after: crate::store::After<crate::store::EventId>,
                    limit: usize,
                ) -> Result<
                    crate::store::multi::Page<crate::store::Event<
                        <$stream as crate::store::Stream>::Id,
                        <$stream as crate::store::Stream>::Payload,
                    >>,
                    Self::Error,
                > {
                    let state = self.state.lock().expect("poisoned");
                    let stream = &state.[<$stream:snake>];
                    let full = stream.select_events.get(&id).cloned().unwrap_or_default();
                    let filtered: Vec<crate::store::Event<
                        <$stream as crate::store::Stream>::Id,
                        <$stream as crate::store::Stream>::Payload,
                    >> = match after {
                        crate::store::After::Start => {
                            full.into_iter().take(limit + 1).collect()
                        }
                        crate::store::After::Specific(event_id) => full
                            .into_iter()
                            .filter(|e| e.event_id > event_id)
                            .take(limit + 1)
                            .collect(),
                    };
                    let items: Vec<_> =
                        filtered.clone().into_iter().take(limit).collect();
                    let more = filtered.len() > limit;
                    let next = items
                        .last()
                        .map(|e| e.event_id)
                        .unwrap_or(state.latest);
                    Ok(crate::store::multi::Page { items, more, next })
                }
            }
        }
    };
}

#[cfg(test)]
pub(crate) use memory_store;

#[cfg(test)]
mod tests {
    use crate::store::Event;
    use crate::store::Stream;

    pub struct TestStream;
    impl Stream for TestStream {
        type Id = u32;
        type Payload = String;
    }

    // Single-stream form — no event enum needed
    memory_store! {
        type TestStore = MemoryStore<TestStream>
    }

    multi_store_tests!(TestStream, Event<u32, String>, TestStore::new());

    #[tokio::test]
    async fn observe_returns_bare_events_without_enum() {
        use crate::authority::Actor;
        use crate::authority::Authority;
        use crate::store::After;
        use crate::store::When;
        use crate::store::multi::Observe;
        use crate::store::multi::Store;
        use chrono::Utc;

        let store = TestStore::new();
        store
            .record(
                Authority::Direct(Actor::System),
                Utc::now(),
                1u32,
                "hello".to_string(),
                When::Empty,
            )
            .await
            .expect("should succeed");

        let page = store
            .observe(After::Start, 10)
            .await
            .expect("should succeed");
        // Access fields directly — no enum match needed
        assert_eq!(page.items[0].id, 1u32);
        assert_eq!(page.items[0].payload, "hello");
    }
}
