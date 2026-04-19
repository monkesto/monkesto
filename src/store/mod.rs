use serde::Deserialize;
use serde::Serialize;
use std::ops::Deref;

pub trait Stream {
    type Id: Send + Sync + Copy + Clone;
    type Payload: Send + Sync + Clone;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EventId(u64);

impl EventId {
    #[cfg_attr(not(test), expect(dead_code))]
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

pub mod multi;
#[expect(dead_code)]
pub mod universal;
