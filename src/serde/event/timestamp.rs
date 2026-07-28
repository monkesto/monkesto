use crate::proto::event::timestamp::ProtoTimestamp;
use crate::serde::error::ProtoError;
use crate::serde::error::ProtoError::FieldRequired;
use crate::time_provider::Timestamp;
use chrono::DateTime;

impl From<Timestamp> for ProtoTimestamp {
    fn from(timestamp: Timestamp) -> Self {
        ProtoTimestamp {
            unix_millis: timestamp.timestamp_millis(),
        }
    }
}

impl TryFrom<ProtoTimestamp> for Timestamp {
    type Error = ProtoError;

    fn try_from(proto_timestamp: ProtoTimestamp) -> Result<Self, Self::Error> {
        Ok(Timestamp(
            DateTime::from_timestamp_millis(proto_timestamp.unix_millis)
                .ok_or(ProtoError::Deserialize)?,
        ))
    }
}

impl TryFrom<Option<ProtoTimestamp>> for Timestamp {
    type Error = ProtoError;

    fn try_from(proto_timestamp: Option<ProtoTimestamp>) -> Result<Self, Self::Error> {
        proto_timestamp.ok_or(FieldRequired)?.try_into()
    }
}
