use chrono::{DateTime, Duration, Utc};
use std::cell::Cell;

pub trait TimeProvider {
    fn get_time(&self) -> DateTime<Utc>;
}

pub struct DefaultTimeProvider;

impl DefaultTimeProvider {
    fn new() -> Self {
        Self
    }
}

impl TimeProvider for DefaultTimeProvider {
    fn get_time(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

pub struct IncrementalTimeProvider {
    current_value: Cell<DateTime<Utc>>,
}

impl IncrementalTimeProvider {
    fn new() -> Self {
        Self {
            current_value: Cell::new(DateTime::UNIX_EPOCH),
        }
    }
}

impl TimeProvider for IncrementalTimeProvider {
    fn get_time(&self) -> DateTime<Utc> {
        let old_value = self.current_value.get();

        // increment the timestamp by one second
        self.current_value
            .update(|t| t + Duration::milliseconds(1000));

        old_value
    }
}
