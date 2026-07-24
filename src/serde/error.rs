use crate::monkesto_error::MonkestoError;
use crate::proto::error::{
    ProtoBalanceUpdate, ProtoIdentError, ProtoJournalError, ProtoMonkestoError, ProtoNameError,
    ProtoUserError, RepeatedBalanceUpdates,
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
}

use crate::authn::user::UserError;
use crate::email::{Email, EmailError};
use crate::id::IdentError;
use crate::journal::transaction::{
    BalanceUpdate, EntryType, TransactionEntries, TransactionValidationError,
};
use crate::journal::{JournalError, PermissionDecodeError, Permissions};
use crate::name::NameError;
use crate::proto::error::proto_balance_update::{ProtoEntryType, proto_entry_type};
use crate::proto::error::proto_decode_error::ProtoErrorType;
use crate::proto::error::proto_ident_error::IdentErrorType;
use crate::proto::error::proto_journal_error::proto_transaction_validation_error::TransactionValidationErrorType;
use crate::proto::error::proto_journal_error::{JournalErrorType, ProtoTransactionValidationError};
use crate::proto::error::proto_monkesto_error::MonkestoErrorType;
use crate::proto::error::proto_name_error::NameErrorType;
use crate::proto::error::proto_user_error::UserErrorType;
use ProtoError::*;

impl TryFrom<RepeatedBalanceUpdates> for TransactionEntries {
    type Error = ProtoError;

    fn try_from(value: RepeatedBalanceUpdates) -> Result<Self, Self::Error> {
        let mut translated_updates = Vec::new();

        for entry in value.updates {
            translated_updates.push(BalanceUpdate {
                account_id: entry.account_id.into(),
                amount: entry.amount,
                entry_type: match entry
                    .entry_type
                    .ok_or(FieldRequired)?
                    .entry_type
                    .ok_or(FieldRequired)?
                {
                    proto_entry_type::EntryType::Credit(_) => EntryType::Credit,
                    proto_entry_type::EntryType::Debit(_) => EntryType::Debit,
                },
            })
        }

        Ok(TransactionEntries(translated_updates))
    }
}

impl From<TransactionEntries> for RepeatedBalanceUpdates {
    fn from(updates: TransactionEntries) -> Self {
        RepeatedBalanceUpdates {
            updates: updates
                .0
                .iter()
                .map(|u| ProtoBalanceUpdate {
                    account_id: u.account_id.to_string(),
                    amount: u.amount,
                    entry_type: Some(match u.entry_type {
                        EntryType::Credit => ProtoEntryType {
                            entry_type: Some(proto_entry_type::EntryType::Credit(())),
                        },
                        EntryType::Debit => ProtoEntryType {
                            entry_type: Some(proto_entry_type::EntryType::Debit(())),
                        },
                    }),
                })
                .collect(),
        }
    }
}

impl TryFrom<ProtoMonkestoError> for MonkestoError {
    type Error = ProtoError;

    fn try_from(proto_error: ProtoMonkestoError) -> Result<Self, Self::Error> {
        let error = match proto_error.monkesto_error_type.ok_or(FieldRequired)? {
            MonkestoErrorType::ErrorDecode(e) => {
                let proto_error = match e.proto_error_type.ok_or(FieldRequired)? {
                    ProtoErrorType::Deserialize(_) => Deserialize,
                    ProtoErrorType::FieldRequired(_) => FieldRequired,
                    ProtoErrorType::PermissionDecode(bits) => PermissionDecode(bits),
                    ProtoErrorType::ParseEmail(em) => ParseEmail(EmailError::RegexViolated(em)),
                };

                MonkestoError::Proto(proto_error)
            }
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
                    JournalErrorType::IdCollision(id) => JournalError::IdCollision(id.into()),
                    JournalErrorType::InvalidJournal(id) => JournalError::InvalidJournal(id.into()),
                    JournalErrorType::Permissions(perms) => JournalError::Permissions(
                        Permissions::from_bits(perms).ok_or(PermissionDecode(perms))?,
                    ),
                    JournalErrorType::UserAlreadyHasAccess(id) => {
                        JournalError::UserAlreadyHasAccess(id.into())
                    }
                    JournalErrorType::UserDoesntHaveAccess(id) => {
                        JournalError::UserDoesntHaveAccess(id.into())
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
                        JournalError::AccountIdCollision(id.into())
                    }
                    JournalErrorType::TransactionIdCollision(id) => {
                        JournalError::TransactionIdCollision(id.into())
                    }
                    JournalErrorType::InvalidAccount(id) => JournalError::InvalidAccount(id.into()),
                    JournalErrorType::InvalidTransaction(id) => {
                        JournalError::InvalidTransaction(id.into())
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
                };

                MonkestoError::Journal(journal_error)
            }
            MonkestoErrorType::User(e) => {
                let user_error = match e.user_error_type.ok_or(FieldRequired)? {
                    UserErrorType::EmailConflict(e) => UserError::EmailConflict(Email::try_new(e)?),
                    UserErrorType::EmailDoesntExist(e) => {
                        UserError::EmailDoesntExist(Email::try_new(e)?)
                    }
                    UserErrorType::IdCollision(id) => UserError::IdCollision(id.into()),
                    UserErrorType::UserDoesntExist(id) => UserError::UserDoesntExist(id.into()),
                    UserErrorType::SessionNotFound(_) => UserError::SessionNotFound,
                    UserErrorType::Sqlx(e) => UserError::Sqlx(e),
                    UserErrorType::SeedFailure(e) => UserError::SeedFailure(Email::try_new(e)?),
                    UserErrorType::PasskeyDecode(s) => UserError::PasskeyDecode(s),
                };

                MonkestoError::User(user_error)
            }
            MonkestoErrorType::DisintegrateEvent(s) => MonkestoError::DisintegrateEvent(s),
            MonkestoErrorType::DisintegrateState(s) => MonkestoError::DisintegrateState(s),
        };

        Ok(error)
    }
}
impl From<MonkestoError> for ProtoMonkestoError {
    fn from(error: MonkestoError) -> Self {
        let e = match error {
            MonkestoError::Proto(e) => {
                let e = match e {
                    FieldRequired => ProtoErrorType::FieldRequired(()),
                    PermissionDecode(bits) => ProtoErrorType::PermissionDecode(bits),
                    ParseEmail(e) => match e {
                        EmailError::RegexViolated(em) => ProtoErrorType::ParseEmail(em),
                    },
                    Deserialize => ProtoErrorType::Deserialize(()),
                };

                MonkestoErrorType::ErrorDecode(crate::proto::error::ProtoDecodeError {
                    proto_error_type: Some(e),
                })
            }
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
                    JournalError::IdCollision(id) => JournalErrorType::IdCollision(id.to_string()),
                    JournalError::AccountIdCollision(id) => {
                        JournalErrorType::AccountIdCollision(id.to_string())
                    }
                    JournalError::TransactionIdCollision(id) => {
                        JournalErrorType::TransactionIdCollision(id.to_string())
                    }
                    JournalError::InvalidJournal(id) => {
                        JournalErrorType::InvalidJournal(id.to_string())
                    }
                    JournalError::InvalidAccount(id) => {
                        JournalErrorType::InvalidAccount(id.to_string())
                    }
                    JournalError::InvalidTransaction(id) => {
                        JournalErrorType::InvalidTransaction(id.to_string())
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
                        JournalErrorType::UserAlreadyHasAccess(id.to_string())
                    }
                    JournalError::UserDoesntHaveAccess(id) => {
                        JournalErrorType::UserDoesntHaveAccess(id.to_string())
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
                    UserError::IdCollision(id) => UserErrorType::IdCollision(id.to_string()),
                    UserError::UserDoesntExist(id) => {
                        UserErrorType::UserDoesntExist(id.to_string())
                    }
                    UserError::SessionNotFound => UserErrorType::SessionNotFound(()),
                    UserError::Sqlx(s) => UserErrorType::Sqlx(s),
                    UserError::SeedFailure(em) => UserErrorType::SeedFailure(em.to_string()),
                    UserError::PasskeyDecode(s) => UserErrorType::PasskeyDecode(s),
                };

                MonkestoErrorType::User(ProtoUserError {
                    user_error_type: Some(e),
                })
            }
            MonkestoError::DisintegrateEvent(s) => MonkestoErrorType::DisintegrateEvent(s),
            MonkestoError::DisintegrateState(s) => MonkestoErrorType::DisintegrateState(s),
        };

        ProtoMonkestoError {
            monkesto_error_type: Some(e),
        }
    }
}
