use crate::journal::activity::ActivityId;
use crate::journal::transaction::TransactionEntries;
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum TransactionValidationError {
    #[error("Received an invalid entry type. Expected Dr or Cr, found {0}")]
    InvalidEntryType(String),
    #[error("Did not receive any transaction entries")]
    NoTransactionEntries,
    #[error("Did not receive a corresponding amount for an entry")]
    MissingEntryAmount,
    #[error("Did not receive a corresponding entry type for an entry")]
    MissingEntryType,
    #[error("Invalid entry amount: {0}")]
    ParseDecimal(String),
    #[error("Received an entry with a partial cent value: {0}")]
    PartialCentValue(String),
    #[error("Received an entry with a value greater than 9 quintillion")]
    OutOfRange(String),
    #[error(
        "Received an entry with a negative amount: {0}. Please use the debit/credit selector instead."
    )]
    NegativeEntryAmount(String),
    #[error("Imbalanced transaction: {:?}", 0)]
    ImbalancedTransaction(TransactionEntries),
    #[error(
        "attempted a transfer involving activity {0}, but it doesn't have a 'transfer' activity type"
    )]
    TransferViolation(ActivityId),
}
