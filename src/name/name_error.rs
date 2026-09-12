use crate::error::DecodeError;
use crate::error::DecodeError::FieldRequired;
use crate::proto::name::name_error::ProtoNameError;
use crate::proto::name::name_error::proto_name_error::NameErrorType;
use thiserror::Error;

#[derive(Error, Debug, Eq, PartialEq)]
pub enum NameError {
    #[error("The name {0} is too short")]
    TooShort(String),

    #[error("The name {0} is too long")]
    TooLong(String),
}

impl TryFrom<ProtoNameError> for NameError {
    type Error = DecodeError;

    fn try_from(name_err: ProtoNameError) -> Result<Self, Self::Error> {
        let err = match name_err.name_error_type.ok_or(FieldRequired)? {
            NameErrorType::TooShort(s) => NameError::TooShort(s.to_string()),
            NameErrorType::TooLong(s) => NameError::TooLong(s),
        };

        Ok(err)
    }
}

impl From<NameError> for ProtoNameError {
    fn from(e: NameError) -> Self {
        let err = match e {
            NameError::TooShort(s) => NameErrorType::TooShort(s.to_string()),
            NameError::TooLong(s) => NameErrorType::TooLong(s.to_string()),
        };

        Self {
            name_error_type: Some(err),
        }
    }
}
