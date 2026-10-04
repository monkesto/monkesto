use crate::authn::UserId;
use crate::authn::user::UserError;
use crate::email::EmailError;
use crate::id::ident_error::IdentError;
use crate::journal::account::AccountId;
use crate::journal::activity::ActivityId;
use crate::journal::entry::EntryId;
use crate::journal::error::transaction_validation::TransactionValidationError;
use crate::journal::file::FileId;
use crate::journal::fund::FundId;
use crate::journal::jewel::JewelImportError;
use crate::journal::transaction::TransactionId;
use crate::journal::transaction::memo::MemoError;
use crate::journal::{JournalId, PermissionDecodeError, Permissions};
use crate::name::NameError;
use crate::proto::DecodeError;
use aws_sdk_s3::error::SdkError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_login::tracing::error;
use disintegrate::{BoxDynError, DecisionError};
use std::backtrace::Backtrace;
use thiserror::Error;

#[derive(Error, Debug)]
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
    #[expect(unused)]
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
    #[expect(unused)]
    InvalidFund(FundId),

    #[error("invalid entry: {0}")]
    InvalidEntry(EntryId),

    #[error("invalid transaction: {0}")]
    #[expect(unused)]
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

    #[error("sqlx returned an error: {0}; backtrace {:#?}", backtrace)]
    Sqlx {
        #[from]
        source: sqlx::Error,
        backtrace: Backtrace,
    },

    #[error("failed to construct permissions from an integer: {0}")]
    PermissionDecode(#[from] PermissionDecodeError),

    #[error("failed to decode an event: {0}")]
    EventDecode(#[from] DecodeError),

    #[error("an S3 transaction failed: {0}")]
    S3(String),

    #[error("invalid file: {0}")]
    InvalidFile(FileId),

    #[error("failed to import from a jewel database: {0}")]
    JewelImport(#[from] JewelImportError),

    #[error("failed to create a memo: {0}")]
    Memo(#[from] MemoError),

    #[error("failed to create a name: {0}")]
    Name(#[from] NameError),

    #[error("got an error from the disintegrate event store: {0}")]
    DisintegrateEvent(BoxDynError),

    #[error("got an error from the disintegrate state store: {0}")]
    DisintegrateState(BoxDynError),

    #[error("got an error from the user store: {0}")]
    User(#[from] UserError),

    #[error(transparent)]
    Email(#[from] EmailError),
}

impl From<DecisionError<JournalError>> for JournalError {
    fn from(error: DecisionError<JournalError>) -> Self {
        match error {
            DecisionError::Domain(e) => e,
            DecisionError::EventStore(e) => JournalError::DisintegrateEvent(e),
            DecisionError::StateStore(e) => JournalError::DisintegrateState(e),
        }
    }
}

impl<E, R> From<SdkError<E, R>> for JournalError {
    fn from(value: SdkError<E, R>) -> Self {
        Self::S3(value.to_string())
    }
}

pub type JournalResult<T> = Result<T, JournalError>;

impl IntoResponse for JournalError {
    fn into_response(self) -> Response {
        use JournalError::*;
        match self {
            EventDecode(_)
            | PermissionDecode(_)
            | Sqlx { .. }
            | S3(_)
            | DisintegrateState(_)
            | DisintegrateEvent(_) => {
                error!("{}", self);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            IdCollision(_)
            | AccountIdCollision(_)
            | ActivityIdCollision(_)
            | FundIdCollision(_)
            | EntryIdCollision(_)
            | TransactionIdCollision(_)
            | FileIdCollision(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Id collision, try again.",
            )
                .into_response(),
            InvalidJournal(_)
            | InvalidAccount(_)
            | InvalidActivity(_)
            | InvalidFund(_)
            | InvalidEntry(_)
            | InvalidTransaction(_)
            | InvalidFile(_) => StatusCode::CONFLICT.into_response(),
            TransactionValidation(e) => e.into_response(),
            Permissions(perms) => (
                StatusCode::UNAUTHORIZED,
                format!("The {:?} permission is required", perms),
            )
                .into_response(),
            UserAlreadyHasAccess(_) => (
                StatusCode::CONFLICT,
                "The user already has access to this journal",
            )
                .into_response(),
            UserDoesntHaveAccess(_) => (
                StatusCode::CONFLICT,
                "The user doesn't have access to this journal",
            )
                .into_response(),
            JewelImport(e) => e.into_response(),
            IdentCreation(_) => (StatusCode::BAD_REQUEST, "Invalid id").into_response(),
            Memo(_) => (
                StatusCode::BAD_REQUEST,
                "Memos must have a maximum of 100 characters",
            )
                .into_response(),
            Name(_) => (
                StatusCode::BAD_REQUEST,
                "Names must have a maximum of 64 characters",
            )
                .into_response(),
            User(e) => e.into_response(),
            Email(_) => (StatusCode::BAD_REQUEST, "Invalid email").into_response(),
        }
    }
}
