use crate::error::DecodeError;
use crate::error::DecodeError::FieldRequired;
use crate::proto::id::ident_error::ProtoIdentError;
use crate::proto::id::ident_error::proto_ident_error::IdentErrorType;
use thiserror::Error;

#[derive(Debug, Error, Clone, Eq, PartialEq)]
pub enum IdentError {
    #[error("Failed to parse the provided bytes: {0}")]
    Parse(String),

    #[error("The provided string is not a valid Ident: {0}")]
    InvalidId(String),
}

impl TryFrom<ProtoIdentError> for IdentError {
    type Error = DecodeError;

    fn try_from(e: ProtoIdentError) -> Result<Self, Self::Error> {
        let err = match e.ident_error_type.ok_or(FieldRequired)? {
            IdentErrorType::Parse(s) => IdentError::Parse(s),
            IdentErrorType::InvalidId(s) => IdentError::InvalidId(s),
        };

        Ok(err)
    }
}

impl From<IdentError> for ProtoIdentError {
    fn from(e: IdentError) -> Self {
        let err = match e {
            IdentError::Parse(s) => IdentErrorType::Parse(s),
            IdentError::InvalidId(s) => IdentErrorType::InvalidId(s),
        };

        Self {
            ident_error_type: Some(err),
        }
    }
}
