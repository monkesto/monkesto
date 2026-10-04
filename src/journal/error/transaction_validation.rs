use crate::journal::entry::EntrySideFromIntError;
use crate::journal::transaction::TransactionEntries;
use crate::journal::transaction::memo::MemoError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum TransactionValidationError {
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
    #[error(transparent)]
    InvalidEntrySide(#[from] EntrySideFromIntError),
    #[error(transparent)]
    Memo(#[from] MemoError),
}

impl IntoResponse for TransactionValidationError {
    fn into_response(self) -> Response {
        use TransactionValidationError::*;

        (
            StatusCode::BAD_REQUEST,
            match self {
                NoTransactionEntries => "No transaction entries",
                MissingEntryAmount => "Missing entry amount",
                MissingEntryType => "Missing entry type",
                ParseDecimal(_) => "Failed to parse a dollar amount",
                PartialCentValue(_) => "Failed to parse a partial cent value",
                OutOfRange(_) => "Values must be below 9 quintillion",
                NegativeEntryAmount(_) => "All entry amounts must be positive",
                ImbalancedTransaction(_) => "The transaction is not balanced",
                InvalidEntrySide(_) => "Invalid entry side",
                Memo(_) => "Memos must be under 100 characters",
            },
        )
            .into_response()
    }
}
