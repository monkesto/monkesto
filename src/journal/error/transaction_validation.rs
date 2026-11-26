use crate::journal::transaction::memo::MemoError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use thiserror::Error;

#[derive(Error, Debug, PartialEq)]
pub enum TransactionValidationError {
    #[error("Did not receive any transaction entries")]
    NoTransactionEntries,
    #[error("Got multiple entries with the same id: {0}")]
    EntryIdCollision(i64),
    #[error("{0}")]
    InvalidJson(String),
    #[error("Invalid entry amount: {0}")]
    ParseDecimal(String),
    #[error("Received an entry with a partial cent value: {0}")]
    PartialCentValue(String),
    #[error("The entry amount is out of range: {0}")]
    OutOfRange(String),
    #[error(
        "Received an entry with a nonpositive amount: {0}. Please use the debit/credit selector instead."
    )]
    NegativeEntryAmount(String),
    #[error("Imbalanced transaction")]
    ImbalancedTransaction,
    #[error(transparent)]
    Memo(#[from] MemoError),
}

impl IntoResponse for TransactionValidationError {
    fn into_response(self) -> Response {
        use TransactionValidationError::*;

        let message = match &self {
            NoTransactionEntries => "Add at least one transaction entry",
            InvalidJson(message) => message,
            ParseDecimal(_) => "Failed to parse a dollar amount",
            PartialCentValue(_) => "Entry amounts must be in whole cents",
            OutOfRange(_) => "An entry amount is too large",
            NegativeEntryAmount(_) => "All entry amounts must be positive",
            ImbalancedTransaction => "The transaction is not balanced.",
            Memo(_) => "Memos must be at most 100 characters",
            EntryIdCollision(_) => "Multiple entries had the same id",
        };
        (StatusCode::BAD_REQUEST, message.to_owned()).into_response()
    }
}
