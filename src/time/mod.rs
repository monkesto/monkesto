#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timestamp(pub DateTime<Utc>);

impl Deref for Timestamp {
    type Target = DateTime<Utc>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

use crate::error::DecodeError;
use crate::error::DecodeError::FieldRequired;
use crate::proto::time::timestamp::ProtoTimestamp;
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

impl From<Timestamp> for ProtoTimestamp {
    fn from(timestamp: Timestamp) -> Self {
        ProtoTimestamp {
            unix_millis: timestamp.timestamp_millis(),
        }
    }
}

impl TryFrom<ProtoTimestamp> for Timestamp {
    type Error = DecodeError;

    fn try_from(proto_timestamp: ProtoTimestamp) -> Result<Self, Self::Error> {
        Ok(Timestamp(
            DateTime::from_timestamp_millis(proto_timestamp.unix_millis)
                .ok_or(DecodeError::Deserialize)?,
        ))
    }
}

impl TryFrom<Option<ProtoTimestamp>> for Timestamp {
    type Error = DecodeError;

    fn try_from(proto_timestamp: Option<ProtoTimestamp>) -> Result<Self, Self::Error> {
        proto_timestamp.ok_or(FieldRequired)?.try_into()
    }
}
