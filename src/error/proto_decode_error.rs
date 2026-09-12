use crate::email::EmailError;
use crate::error::DecodeError::{
    AccountTypeFromInt, ActivityTypeFromInt, Deserialize, EntrySideFromInt, FieldRequired,
    FinancialPeriodFromInt, Ident, ParseEmail, ParseName, ParseUuid, PermissionDecode,
};
use crate::id::ident_error::IdentError;
use crate::journal::account::AccountTypeFromIntError;
use crate::journal::activity::ActivityTypeFromIntError;
use crate::journal::entry::EntrySideFromIntError;
use crate::journal::transaction::FinancialPeriodFromIntError;
use crate::name::name_error::NameError;
use crate::proto::error::decode_error::ProtoDecodeError;
use crate::proto::error::decode_error::proto_decode_error::ProtoErrorType;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum DecodeError {
    #[error("failed to deserialize the error")]
    Deserialize,
    #[error("expected a field that was missing")]
    FieldRequired,
    #[error("Failed to decode permissions from bits: {0}")]
    PermissionDecode(i32),
    #[error("Failed to parse an email: {0}")]
    ParseEmail(#[from] EmailError),
    #[error("Failed to parse a name: {0}")]
    ParseName(#[from] NameError),
    #[error("Invalid ident: {0}")]
    Ident(#[from] IdentError),
    #[error("invalid webauthn-uuid: {0}")]
    ParseUuid(String),
    #[error("failed to parse an AccountType from the int {0}")]
    AccountTypeFromInt(#[from] AccountTypeFromIntError),
    #[error("failed to parse a FinancialPeriod from the int {0}")]
    FinancialPeriodFromInt(#[from] FinancialPeriodFromIntError),
    #[error("failed to parse an EntrySide from the int {0}")]
    EntrySideFromInt(#[from] EntrySideFromIntError),
    #[error("failed to parse an ActivityType from the int {0}")]
    ActivityTypeFromInt(#[from] ActivityTypeFromIntError),
}

impl From<DecodeError> for ProtoDecodeError {
    fn from(e: DecodeError) -> Self {
        let e = match e {
            FieldRequired => ProtoErrorType::FieldRequired(()),
            PermissionDecode(bits) => ProtoErrorType::PermissionDecode(bits),
            ParseEmail(e) => match e {
                EmailError::RegexViolated(em) => ProtoErrorType::ParseEmail(em),
            },
            Deserialize => ProtoErrorType::Deserialize(()),
            Ident(e) => ProtoErrorType::Ident(e.into()),
            ParseUuid(s) => ProtoErrorType::Uuid(s),
            ParseName(e) => ProtoErrorType::Name(e.into()),
            AccountTypeFromInt(i) => ProtoErrorType::AccountTypeFromInt(i.0 as i32),
            FinancialPeriodFromInt(i) => ProtoErrorType::AccountTypeFromInt(i.0 as i32),
            EntrySideFromInt(i) => ProtoErrorType::EntrySideFromInt(i.0 as i32),
            ActivityTypeFromInt(i) => ProtoErrorType::ActivityTypeFromInt(i.0 as i32),
        };

        ProtoDecodeError {
            proto_error_type: Some(e),
        }
    }
}

impl TryFrom<ProtoDecodeError> for DecodeError {
    type Error = DecodeError;

    fn try_from(e: ProtoDecodeError) -> Result<Self, Self::Error> {
        let proto_error = match e.proto_error_type.ok_or(FieldRequired)? {
            ProtoErrorType::Deserialize(_) => Deserialize,
            ProtoErrorType::FieldRequired(_) => FieldRequired,
            ProtoErrorType::PermissionDecode(bits) => PermissionDecode(bits),
            ProtoErrorType::ParseEmail(em) => ParseEmail(EmailError::RegexViolated(em)),
            ProtoErrorType::Ident(e) => Ident(e.try_into()?),
            ProtoErrorType::Uuid(s) => ParseUuid(s),
            ProtoErrorType::Name(e) => ParseName(e.try_into()?),
            ProtoErrorType::AccountTypeFromInt(i) => AccountTypeFromIntError(i as i8).into(),
            ProtoErrorType::FinancialPeriodFromInt(i) => {
                FinancialPeriodFromIntError(i as i8).into()
            }
            ProtoErrorType::EntrySideFromInt(i) => EntrySideFromIntError(i as i8).into(),
            ProtoErrorType::ActivityTypeFromInt(i) => ActivityTypeFromIntError(i as i8).into(),
        };

        Ok(proto_error)
    }
}
