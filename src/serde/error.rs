use crate::monkesto_error::MonkestoError;
use proto::error::{
    ProtoDecodeError, ProtoIdentError, ProtoJournalError, ProtoMonkestoError, ProtoNameError,
    ProtoPasskeyError, ProtoUserError,
};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ProtoError {
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
}

use crate::authn::passkey::PasskeyError;
use crate::authn::user::UserError;
use crate::email::{Email, EmailError};
use crate::id::IdentError;
use crate::journal::transaction::TransactionValidationError;
use crate::journal::{JournalError, PermissionDecodeError, Permissions};
use crate::name::NameError;
use ProtoError::*;
use proto::error::proto_decode_error::ProtoErrorType;
use proto::error::proto_ident_error::IdentErrorType;
use proto::error::proto_journal_error::proto_transaction_validation_error::TransactionValidationErrorType;
use proto::error::proto_journal_error::{JournalErrorType, ProtoTransactionValidationError};
use proto::error::proto_monkesto_error::MonkestoErrorType;
use proto::error::proto_name_error::NameErrorType;
use proto::error::proto_passkey_error::PasskeyErrorType;
use proto::error::proto_user_error::UserErrorType;

impl From<ProtoError> for ProtoDecodeError {
    fn from(e: ProtoError) -> Self {
        let e = match e {
            FieldRequired => ProtoErrorType::FieldRequired(()),
            PermissionDecode(bits) => ProtoErrorType::PermissionDecode(bits),
            ParseEmail(e) => match e {
                EmailError::RegexViolated(em) => ProtoErrorType::ParseEmail(em),
            },
            Deserialize => ProtoErrorType::Deserialize(()),
            Ident(e) => {
                let ident_error = match e {
                    IdentError::Parse(s) => IdentErrorType::Parse(s),
                    IdentError::InvalidId(s) => IdentErrorType::InvalidId(s),
                };
                ProtoErrorType::Ident(ProtoIdentError {
                    ident_error_type: Some(ident_error),
                })
            }
            ParseUuid(s) => ProtoErrorType::Uuid(s),
            ParseName(e) => {
                let e = match e {
                    NameError::TooShort(s) => NameErrorType::TooShort(s),
                    NameError::TooLong(s) => NameErrorType::TooLong(s),
                };
                ProtoErrorType::Name(ProtoNameError {
                    name_error_type: Some(e),
                })
            }
        };

        ProtoDecodeError {
            proto_error_type: Some(e),
        }
    }
}

impl TryFrom<ProtoDecodeError> for ProtoError {
    type Error = ProtoError;

    fn try_from(e: ProtoDecodeError) -> Result<Self, Self::Error> {
        let proto_error = match e.proto_error_type.ok_or(FieldRequired)? {
            ProtoErrorType::Deserialize(_) => Deserialize,
            ProtoErrorType::FieldRequired(_) => FieldRequired,
            ProtoErrorType::PermissionDecode(bits) => PermissionDecode(bits),
            ProtoErrorType::ParseEmail(em) => ParseEmail(EmailError::RegexViolated(em)),
            ProtoErrorType::Ident(e) => Ident(match e.ident_error_type.ok_or(FieldRequired)? {
                IdentErrorType::Parse(s) => IdentError::Parse(s),
                IdentErrorType::InvalidId(s) => IdentError::InvalidId(s),
            }),
            ProtoErrorType::Uuid(s) => ParseUuid(s),
            ProtoErrorType::Name(e) => {
                let e = match e.name_error_type.ok_or(FieldRequired)? {
                    NameErrorType::TooShort(s) => NameError::TooShort(s),
                    NameErrorType::TooLong(s) => NameError::TooLong(s),
                };
                ParseName(e)
            }
        };

        Ok(proto_error)
    }
}

impl TryFrom<ProtoMonkestoError> for MonkestoError {
    type Error = ProtoError;

    fn try_from(proto_error: ProtoMonkestoError) -> Result<Self, Self::Error> {
        let error = match proto_error.monkesto_error_type.ok_or(FieldRequired)? {
            MonkestoErrorType::ErrorDecode(e) => MonkestoError::Proto(e.try_into()?),
            MonkestoErrorType::NameCreation(e) => match e.name_error_type.ok_or(FieldRequired)? {
                NameErrorType::TooShort(s) => MonkestoError::NameCreation(NameError::TooShort(s)),
                NameErrorType::TooLong(s) => MonkestoError::NameCreation(NameError::TooLong(s)),
            },
            MonkestoErrorType::IdentCreation(e) => {
                match e.ident_error_type.ok_or(FieldRequired)? {
                    IdentErrorType::Parse(s) => MonkestoError::IdentCreation(IdentError::Parse(s)),
                    IdentErrorType::InvalidId(s) => {
                        MonkestoError::IdentCreation(IdentError::InvalidId(s))
                    }
                }
            }
            MonkestoErrorType::EmailCreation(e) => {
                MonkestoError::EmailCreation(EmailError::RegexViolated(e))
            }
            MonkestoErrorType::Journal(e) => {
                let journal_error = match e.journal_error_type.ok_or(FieldRequired)? {
                    JournalErrorType::IdCollision(id) => JournalError::IdCollision(id.try_into()?),
                    JournalErrorType::InvalidJournal(id) => {
                        JournalError::InvalidJournal(id.try_into()?)
                    }
                    JournalErrorType::Permissions(perms) => JournalError::Permissions(
                        Permissions::from_bits(perms).ok_or(PermissionDecode(perms))?,
                    ),
                    JournalErrorType::UserAlreadyHasAccess(id) => {
                        JournalError::UserAlreadyHasAccess(id.try_into()?)
                    }
                    JournalErrorType::UserDoesntHaveAccess(id) => {
                        JournalError::UserDoesntHaveAccess(id.try_into()?)
                    }
                    JournalErrorType::IdentCreation(e) => {
                        match e.ident_error_type.ok_or(FieldRequired)? {
                            IdentErrorType::Parse(s) => {
                                JournalError::IdentCreation(IdentError::Parse(s))
                            }
                            IdentErrorType::InvalidId(s) => {
                                JournalError::IdentCreation(IdentError::InvalidId(s))
                            }
                        }
                    }
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
                    JournalErrorType::InvalidAccount(id) => {
                        JournalError::InvalidAccount(id.try_into()?)
                    }
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
                                TransactionValidationErrorType::ImbalancedTransaction(updates) => {
                                    TransactionValidationError::ImbalancedTransaction(
                                        updates.try_into()?,
                                    )
                                }
                            };

                        JournalError::TransactionValidation(validation_error)
                    }
                    JournalErrorType::ProtoDecode(e) => JournalError::ProtoDecode(e.try_into()?),
                };

                MonkestoError::Journal(journal_error)
            }
            MonkestoErrorType::User(e) => {
                let user_error = match e.user_error_type.ok_or(FieldRequired)? {
                    UserErrorType::EmailConflict(e) => UserError::EmailConflict(Email::try_new(e)?),
                    UserErrorType::EmailDoesntExist(e) => {
                        UserError::EmailDoesntExist(Email::try_new(e)?)
                    }
                    UserErrorType::IdCollision(id) => UserError::IdCollision(id.try_into()?),
                    UserErrorType::UserDoesntExist(id) => {
                        UserError::UserDoesntExist(id.try_into()?)
                    }
                    UserErrorType::SessionNotFound(_) => UserError::SessionNotFound,
                    UserErrorType::Sqlx(e) => UserError::Sqlx(e),
                    UserErrorType::SeedFailure(e) => UserError::SeedFailure(Email::try_new(e)?),
                    UserErrorType::PasskeyDecode(s) => UserError::PasskeyDecode(s),
                    UserErrorType::InvalidInput(_) => UserError::InvalidInput,
                    UserErrorType::SerdeJson(s) => UserError::SerdeJson(s),

                    UserErrorType::Session(s) => UserError::Session(s),
                    UserErrorType::AuthenticationFailed(_) => UserError::AuthenticationFailed,
                    UserErrorType::MissingResendApiKey(_) => UserError::MissingResendApiKey,
                    UserErrorType::Resend(s) => UserError::Resend(s),
                };

                MonkestoError::User(user_error)
            }
            MonkestoErrorType::DisintegrateEvent(s) => MonkestoError::DisintegrateEvent(s),
            MonkestoErrorType::DisintegrateState(s) => MonkestoError::DisintegrateState(s),
            MonkestoErrorType::Passkey(e) => {
                let passkey_error = match e.passkey_error_type.ok_or(FieldRequired)? {
                    PasskeyErrorType::SessionExpired(_) => PasskeyError::SessionExpired,

                    PasskeyErrorType::InvalidInput(_) => PasskeyError::InvalidInput,

                    PasskeyErrorType::SessionError(s) => PasskeyError::SessionError(s),
                    PasskeyErrorType::IdConflict(id) => PasskeyError::IdConflict(id.try_into()?),
                    PasskeyErrorType::PasskeyDoesntExist(id) => {
                        PasskeyError::PasskeyDoesntExist(id.try_into()?)
                    }
                    PasskeyErrorType::UserDoesntExist(id) => {
                        PasskeyError::UserDoesntExist(id.try_into()?)
                    }
                    PasskeyErrorType::Json(s) => PasskeyError::Json(s),
                    PasskeyErrorType::Sqlx(s) => PasskeyError::Sqlx(s),
                };

                MonkestoError::Passkey(passkey_error)
            }
        };

        Ok(error)
    }
}
impl From<MonkestoError> for ProtoMonkestoError {
    fn from(error: MonkestoError) -> Self {
        let e = match error {
            MonkestoError::Proto(e) => MonkestoErrorType::ErrorDecode(e.into()),
            MonkestoError::NameCreation(e) => {
                let e = match e {
                    NameError::TooShort(s) => NameErrorType::TooShort(s),
                    NameError::TooLong(s) => NameErrorType::TooLong(s),
                };

                MonkestoErrorType::NameCreation(ProtoNameError {
                    name_error_type: Some(e),
                })
            }
            MonkestoError::IdentCreation(e) => {
                let e = match e {
                    IdentError::Parse(s) => IdentErrorType::Parse(s),
                    IdentError::InvalidId(s) => IdentErrorType::InvalidId(s),
                };

                MonkestoErrorType::IdentCreation(ProtoIdentError {
                    ident_error_type: Some(e),
                })
            }
            MonkestoError::EmailCreation(EmailError::RegexViolated(s)) => {
                MonkestoErrorType::EmailCreation(s)
            }
            MonkestoError::Journal(e) => {
                let e = match e {
                    JournalError::IdCollision(id) => JournalErrorType::IdCollision(id.into()),
                    JournalError::AccountIdCollision(id) => {
                        JournalErrorType::AccountIdCollision(id.into())
                    }
                    JournalError::TransactionIdCollision(id) => {
                        JournalErrorType::TransactionIdCollision(id.into())
                    }
                    JournalError::InvalidJournal(id) => JournalErrorType::InvalidJournal(id.into()),
                    JournalError::InvalidAccount(id) => JournalErrorType::InvalidAccount(id.into()),
                    JournalError::InvalidTransaction(id) => {
                        JournalErrorType::InvalidTransaction(id.into())
                    }
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
                                TransactionValidationErrorType::ImbalancedTransaction(
                                    updates.into(),
                                )
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
                    JournalError::IdentCreation(e) => {
                        let e = match e {
                            IdentError::Parse(s) => IdentErrorType::Parse(s),
                            IdentError::InvalidId(s) => IdentErrorType::InvalidId(s),
                        };

                        JournalErrorType::IdentCreation(ProtoIdentError {
                            ident_error_type: Some(e),
                        })
                    }
                    JournalError::Sqlx(s) => JournalErrorType::Sqlx(s),
                    JournalError::PermissionDecode(e) => JournalErrorType::PermissionDecode(e.0),
                    JournalError::EventDecode(s) => JournalErrorType::EventDecode(s),
                    JournalError::ProtoDecode(e) => JournalErrorType::ProtoDecode(e.into()),
                };

                MonkestoErrorType::Journal(ProtoJournalError {
                    journal_error_type: Some(e),
                })
            }
            MonkestoError::User(e) => {
                let e = match e {
                    UserError::EmailConflict(em) => UserErrorType::EmailConflict(em.to_string()),
                    UserError::EmailDoesntExist(em) => {
                        UserErrorType::EmailDoesntExist(em.to_string())
                    }
                    UserError::IdCollision(id) => UserErrorType::IdCollision(id.into()),
                    UserError::UserDoesntExist(id) => UserErrorType::UserDoesntExist(id.into()),
                    UserError::SessionNotFound => UserErrorType::SessionNotFound(()),
                    UserError::Sqlx(s) => UserErrorType::Sqlx(s),
                    UserError::SeedFailure(em) => UserErrorType::SeedFailure(em.to_string()),
                    UserError::PasskeyDecode(s) => UserErrorType::PasskeyDecode(s),
                    UserError::InvalidInput => UserErrorType::InvalidInput(()),
                    UserError::SerdeJson(s) => UserErrorType::SerdeJson(s),
                    UserError::Session(s) => UserErrorType::Session(s),
                    UserError::AuthenticationFailed => UserErrorType::AuthenticationFailed(()),
                    UserError::MissingResendApiKey => UserErrorType::MissingResendApiKey(()),
                    UserError::Resend(s) => UserErrorType::Resend(s),
                };

                MonkestoErrorType::User(ProtoUserError {
                    user_error_type: Some(e),
                })
            }
            MonkestoError::DisintegrateEvent(s) => MonkestoErrorType::DisintegrateEvent(s),
            MonkestoError::DisintegrateState(s) => MonkestoErrorType::DisintegrateState(s),
            MonkestoError::Passkey(e) => {
                let e = match e {
                    PasskeyError::SessionExpired => PasskeyErrorType::SessionExpired(()),
                    PasskeyError::InvalidInput => PasskeyErrorType::InvalidInput(()),
                    PasskeyError::SessionError(s) => PasskeyErrorType::SessionError(s),
                    PasskeyError::IdConflict(id) => PasskeyErrorType::IdConflict(id.into()),
                    PasskeyError::PasskeyDoesntExist(id) => {
                        PasskeyErrorType::PasskeyDoesntExist(id.into())
                    }
                    PasskeyError::UserDoesntExist(id) => {
                        PasskeyErrorType::UserDoesntExist(id.into())
                    }
                    PasskeyError::Json(s) => PasskeyErrorType::Json(s),
                    PasskeyError::Sqlx(s) => PasskeyErrorType::Sqlx(s),
                };
                MonkestoErrorType::Passkey(ProtoPasskeyError {
                    passkey_error_type: Some(e),
                })
            }
        };

        ProtoMonkestoError {
            monkesto_error_type: Some(e),
        }
    }
}
