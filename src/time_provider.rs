#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timestamp(pub DateTime<Utc>);

impl Deref for Timestamp {
    type Target = DateTime<Utc>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::ops::Deref;

pub trait TimeProvider {
    fn get_time(&self) -> Timestamp;
}

pub struct DefaultTimeProvider;

#[expect(unused)]
impl DefaultTimeProvider {
    fn new() -> Self {
        Self
    }
}

impl TimeProvider for DefaultTimeProvider {
    fn get_time(&self) -> Timestamp {
        Timestamp(Utc::now())
    }
}

pub struct IncrementalTimeProvider {
    current_value: Cell<DateTime<Utc>>,
}

impl IncrementalTimeProvider {
    pub fn new() -> Self {
        Self {
            current_value: Cell::new(DateTime::UNIX_EPOCH),
        }
    }
}

impl TimeProvider for IncrementalTimeProvider {
    fn get_time(&self) -> Timestamp {
        let old_value = self.current_value.get();

        // increment the timestamp by one second
        self.current_value
            .update(|t| t + Duration::milliseconds(1000));

        Timestamp(old_value)
    }
}

impl TimeProvider for DateTime<Utc> {
    fn get_time(&self) -> Timestamp {
        Timestamp(*self)
    }
}
