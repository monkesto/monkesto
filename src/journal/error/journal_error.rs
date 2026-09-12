use aws_sdk_s3::error::SdkError;
use sqlx::Error;
use thiserror::Error;
use crate::authn::UserId;
use crate::error::DecodeError;
use crate::error::DecodeError::{FieldRequired, PermissionDecode};
use crate::id::ident_error::IdentError;
use crate::journal::account::AccountId;
use crate::journal::activity::ActivityId;
use crate::journal::entry::EntryId;
use crate::journal::file::FileId;
use crate::journal::fund::FundId;
use crate::journal::{JournalId, PermissionDecodeError, Permissions};
use crate::journal::error::transaction_validation_error::TransactionValidationError;
use crate::journal::jewel::JewelImportError;
use crate::journal::transaction::TransactionId;
use crate::proto::journal::error::journal_error::proto_journal_error::{JournalErrorType, ProtoJewelImportError, ProtoTransactionValidationError};
use crate::proto::journal::error::journal_error::proto_journal_error::proto_jewel_import_error::JewelImportErrorType;
use crate::proto::journal::error::journal_error::proto_journal_error::proto_transaction_validation_error::TransactionValidationErrorType;
use crate::proto::journal::error::journal_error::ProtoJournalError;

#[derive(Error, Debug, PartialEq)]
pub enum JournalError {
    #[error("a journal already exists with the id {0}")]
    IdCollision(JournalId),

    #[error("an account already exists with the id {0}")]
    AccountIdCollision(AccountId),

    #[error("an activity already exists with the id {0}")]
    ActivityIdCollision(ActivityId),

    #[error("a fund already exists with the id {0}")]
    FundIdCollision(FundId),

    #[error("an entry already exists with the id {0}")]
    EntryIdCollision(EntryId),

    #[error("a transaction already exists with the id {0}")]
    TransactionIdCollision(TransactionId),

    #[error("a file already exists with the id {0}")]
    FileIdCollision(FileId),

    #[error("invalid journal: {0}")]
    InvalidJournal(JournalId),

    #[error("invalid account: {0}")]
    InvalidAccount(AccountId),

    #[error("invalid activity: {0}")]
    InvalidActivity(ActivityId),

    #[error("invalid fund: {0}")]
    InvalidFund(FundId),

    #[error("invalid entry: {0}")]
    InvalidEntry(EntryId),

    #[error("invalid transaction: {0}")]
    InvalidTransaction(TransactionId),

    #[error("failed to validate a transaction: {0}")]
    TransactionValidation(#[from] TransactionValidationError),

    #[error("The user doesn't have the {:?} permission", .0)]
    Permissions(Permissions),

    #[error("The user {0} already has access to this journal")]
    UserAlreadyHasAccess(UserId),

    #[error("The user {0} doesn't have access to this journal")]
    UserDoesntHaveAccess(UserId),

    #[error("Failed to create an Ident: {0}")]
    IdentCreation(#[from] IdentError),

    #[error("sqlx returned an error: {0}")]
    Sqlx(String),

    #[error("failed to construct permissions from an integer: {0}")]
    PermissionDecode(#[from] PermissionDecodeError),

    #[error("failed to decode an event: {0}")]
    EventDecode(String),

    #[error("failed to decode a proto type: {0}")]
    ProtoDecode(#[from] DecodeError),

    #[error("the server-side S3 credentials are invalid")]
    InvalidS3Credentials,

    #[error("an S3 transaction failed: {0}")]
    S3(String),

    #[error("invalid file: {0}")]
    InvalidFile(FileId),

    #[error("failed to import from a jewel database: {0}")]
    JewelImport(#[from] JewelImportError),
}

impl From<sqlx::Error> for JournalError {
    fn from(value: Error) -> Self {
        Self::Sqlx(value.to_string())
    }
}

impl From<prost::DecodeError> for JournalError {
    fn from(value: prost::DecodeError) -> Self {
        Self::EventDecode(value.to_string())
    }
}

impl<E, R> From<SdkError<E, R>> for JournalError {
    fn from(value: SdkError<E, R>) -> Self {
        Self::S3(value.to_string())
    }
}

pub type JournalResult<T> = Result<T, JournalError>;

impl TryFrom<ProtoJournalError> for JournalError {
    type Error = DecodeError;

    fn try_from(e: ProtoJournalError) -> Result<Self, Self::Error> {
        let journal_error = match e.journal_error_type.ok_or(FieldRequired)? {
            JournalErrorType::IdCollision(id) => JournalError::IdCollision(id.try_into()?),
            JournalErrorType::InvalidJournal(id) => JournalError::InvalidJournal(id.try_into()?),
            JournalErrorType::Permissions(perms) => JournalError::Permissions(
                Permissions::from_bits(perms).ok_or(PermissionDecode(perms))?,
            ),
            JournalErrorType::UserAlreadyHasAccess(id) => {
                JournalError::UserAlreadyHasAccess(id.try_into()?)
            }
            JournalErrorType::UserDoesntHaveAccess(id) => {
                JournalError::UserDoesntHaveAccess(id.try_into()?)
            }
            JournalErrorType::IdentCreation(e) => JournalError::IdentCreation(e.try_into()?),
            JournalErrorType::Sqlx(s) => JournalError::Sqlx(s),
            JournalErrorType::PermissionDecode(e) => {
                JournalError::PermissionDecode(PermissionDecodeError(e))
            }
            JournalErrorType::AccountIdCollision(id) => {
                JournalError::AccountIdCollision(id.try_into()?)
            }
            JournalErrorType::TransactionIdCollision(id) => {
                JournalError::TransactionIdCollision(id.try_into()?)
            }
            JournalErrorType::InvalidAccount(id) => JournalError::InvalidAccount(id.try_into()?),
            JournalErrorType::InvalidTransaction(id) => {
                JournalError::InvalidTransaction(id.try_into()?)
            }
            JournalErrorType::EventDecode(s) => JournalError::EventDecode(s),

            JournalErrorType::TransactionValidation(e) => {
                let validation_error =
                    match e.transaction_validation_error_type.ok_or(FieldRequired)? {
                        TransactionValidationErrorType::InvalidEntryType(s) => {
                            TransactionValidationError::InvalidEntryType(s)
                        }
                        TransactionValidationErrorType::NoTransactionEntries(_) => {
                            TransactionValidationError::NoTransactionEntries
                        }
                        TransactionValidationErrorType::MissingEntryAmount(_) => {
                            TransactionValidationError::MissingEntryAmount
                        }
                        TransactionValidationErrorType::MissingEntryType(_) => {
                            TransactionValidationError::MissingEntryType
                        }
                        TransactionValidationErrorType::ParseDecimal(s) => {
                            TransactionValidationError::ParseDecimal(s)
                        }
                        TransactionValidationErrorType::PartialCentValue(s) => {
                            TransactionValidationError::PartialCentValue(s)
                        }
                        TransactionValidationErrorType::OutOfRange(s) => {
                            TransactionValidationError::OutOfRange(s)
                        }
                        TransactionValidationErrorType::NegativeEntryAmount(s) => {
                            TransactionValidationError::NegativeEntryAmount(s)
                        }
                        TransactionValidationErrorType::ImbalancedTransaction(entries) => {
                            TransactionValidationError::ImbalancedTransaction(entries.try_into()?)
                        }
                        TransactionValidationErrorType::TransferViolation(id) => {
                            TransactionValidationError::TransferViolation(id.try_into()?)
                        }
                    };

                JournalError::TransactionValidation(validation_error)
            }
            JournalErrorType::ProtoDecode(e) => JournalError::ProtoDecode(e.try_into()?),
            JournalErrorType::FileIdCollision(id) => JournalError::FileIdCollision(id.try_into()?),
            JournalErrorType::InvalidS3Credentials(_) => JournalError::InvalidS3Credentials,
            JournalErrorType::S3(s) => JournalError::S3(s),
            JournalErrorType::InvalidFile(id) => JournalError::InvalidFile(id.try_into()?),
            JournalErrorType::FundIdCollision(id) => JournalError::FundIdCollision(id.try_into()?),
            JournalErrorType::InvalidFund(id) => JournalError::InvalidFund(id.try_into()?),
            JournalErrorType::ActivityIdCollision(id) => {
                JournalError::ActivityIdCollision(id.try_into()?)
            }
            JournalErrorType::InvalidActivity(id) => JournalError::InvalidActivity(id.try_into()?),
            JournalErrorType::InvalidTransactionEntry(id) => {
                JournalError::InvalidEntry(id.try_into()?)
            }
            JournalErrorType::TransactionEntryIdCollision(id) => {
                JournalError::EntryIdCollision(id.try_into()?)
            }
            JournalErrorType::JewelImport(e) => {
                let import_error = match e.jewel_import_error_type.ok_or(FieldRequired)? {
                    JewelImportErrorType::Io(s) => JewelImportError::Io(s),
                    JewelImportErrorType::StartChildProcess(s) => {
                        JewelImportError::StartChildProcess(s)
                    }
                    JewelImportErrorType::Write(s) => JewelImportError::Write(s),
                    JewelImportErrorType::ChildProcessExitFailure(s) => {
                        JewelImportError::ChildProcessExitFailure(s)
                    }
                    JewelImportErrorType::OutdatedJewelVersion(s) => {
                        JewelImportError::OutdatedJewelVersion(s as f64)
                    }
                };
                JournalError::JewelImport(import_error)
            }
        };

        Ok(journal_error)
    }
}

impl From<JournalError> for ProtoJournalError {
    fn from(e: JournalError) -> Self {
        let err = match e {
            JournalError::IdCollision(id) => JournalErrorType::IdCollision(id.into()),
            JournalError::AccountIdCollision(id) => JournalErrorType::AccountIdCollision(id.into()),
            JournalError::TransactionIdCollision(id) => {
                JournalErrorType::TransactionIdCollision(id.into())
            }
            JournalError::InvalidJournal(id) => JournalErrorType::InvalidJournal(id.into()),
            JournalError::InvalidAccount(id) => JournalErrorType::InvalidAccount(id.into()),
            JournalError::InvalidTransaction(id) => JournalErrorType::InvalidTransaction(id.into()),
            JournalError::TransactionValidation(e) => {
                let t_val = match e {
                    TransactionValidationError::InvalidEntryType(s) => {
                        TransactionValidationErrorType::InvalidEntryType(s)
                    }
                    TransactionValidationError::NoTransactionEntries => {
                        TransactionValidationErrorType::NoTransactionEntries(())
                    }
                    TransactionValidationError::MissingEntryAmount => {
                        TransactionValidationErrorType::MissingEntryAmount(())
                    }
                    TransactionValidationError::MissingEntryType => {
                        TransactionValidationErrorType::MissingEntryType(())
                    }
                    TransactionValidationError::ParseDecimal(s) => {
                        TransactionValidationErrorType::ParseDecimal(s)
                    }
                    TransactionValidationError::PartialCentValue(s) => {
                        TransactionValidationErrorType::PartialCentValue(s)
                    }
                    TransactionValidationError::OutOfRange(s) => {
                        TransactionValidationErrorType::OutOfRange(s)
                    }
                    TransactionValidationError::NegativeEntryAmount(s) => {
                        TransactionValidationErrorType::NegativeEntryAmount(s)
                    }
                    TransactionValidationError::ImbalancedTransaction(updates) => {
                        TransactionValidationErrorType::ImbalancedTransaction(updates.into())
                    }
                    TransactionValidationError::TransferViolation(id) => {
                        TransactionValidationErrorType::TransferViolation(id.into())
                    }
                };
                JournalErrorType::TransactionValidation(ProtoTransactionValidationError {
                    transaction_validation_error_type: Some(t_val),
                })
            }
            JournalError::Permissions(perms) => JournalErrorType::Permissions(perms.bits()),
            JournalError::UserAlreadyHasAccess(id) => {
                JournalErrorType::UserAlreadyHasAccess(id.into())
            }
            JournalError::UserDoesntHaveAccess(id) => {
                JournalErrorType::UserDoesntHaveAccess(id.into())
            }
            JournalError::IdentCreation(e) => JournalErrorType::IdentCreation(e.into()),
            JournalError::Sqlx(s) => JournalErrorType::Sqlx(s),
            JournalError::PermissionDecode(e) => JournalErrorType::PermissionDecode(e.0),
            JournalError::EventDecode(s) => JournalErrorType::EventDecode(s),
            JournalError::ProtoDecode(e) => JournalErrorType::ProtoDecode(e.into()),
            JournalError::FileIdCollision(id) => JournalErrorType::FileIdCollision(id.into()),
            JournalError::InvalidS3Credentials => JournalErrorType::InvalidS3Credentials(()),
            JournalError::S3(s) => JournalErrorType::S3(s),
            JournalError::InvalidFile(id) => JournalErrorType::InvalidFile(id.into()),
            JournalError::FundIdCollision(id) => JournalErrorType::FileIdCollision(id.into()),
            JournalError::InvalidFund(id) => JournalErrorType::InvalidFund(id.into()),
            JournalError::ActivityIdCollision(id) => {
                JournalErrorType::ActivityIdCollision(id.into())
            }
            JournalError::InvalidActivity(id) => JournalErrorType::InvalidActivity(id.into()),
            JournalError::EntryIdCollision(id) => {
                JournalErrorType::TransactionEntryIdCollision(id.into())
            }
            JournalError::InvalidEntry(id) => JournalErrorType::InvalidTransactionEntry(id.into()),
            JournalError::JewelImport(e) => {
                let import_error = match e {
                    JewelImportError::Io(s) => JewelImportErrorType::Io(s),
                    JewelImportError::StartChildProcess(s) => {
                        JewelImportErrorType::StartChildProcess(s)
                    }
                    JewelImportError::Write(s) => JewelImportErrorType::Write(s),
                    JewelImportError::ChildProcessExitFailure(s) => {
                        JewelImportErrorType::ChildProcessExitFailure(s)
                    }
                    JewelImportError::OutdatedJewelVersion(f) => {
                        JewelImportErrorType::OutdatedJewelVersion(f as f32)
                    }
                };

                JournalErrorType::JewelImport(ProtoJewelImportError {
                    jewel_import_error_type: Some(import_error),
                })
            }
        };

        Self {
            journal_error_type: Some(err),
        }
    }
}
