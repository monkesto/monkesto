use crate::proto::journal::transaction::memo::memo_error::ProtoMemoError;
use thiserror::Error;

#[derive(Error, Debug, Eq, PartialEq)]
#[error("The memo {0} is too long; max length is 100 characters")]
pub struct MemoError(pub String);

impl From<ProtoMemoError> for MemoError {
    fn from(memo_err: ProtoMemoError) -> Self {
        Self(memo_err.error)
    }
}

impl From<MemoError> for ProtoMemoError {
    fn from(e: MemoError) -> Self {
        Self { error: e.0 }
    }
}
