use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::journal::JournalId;
use crate::journal::activity::scope::ActivityScope;
use crate::journal::error::{JournalError, TransactionValidationError};
use crate::journal::transaction::entry::{EntryKind, EntrySide};
use crate::journal::transaction::memo::Memo;
use crate::journal::transaction::{FinancialPeriod, TransactionEntry, TransactionId};
use crate::time::{DefaultTimeProvider, TimeProvider};
use crate::{BackendType, StateType};
use axum::Json;
use axum::extract::rejection::JsonRejection;
use axum::extract::{Path, State};
use axum::response::Redirect;
use axum_login::AuthSession;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::{Decimal, dec};
use serde::Deserialize;
use std::str::FromStr;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransactRequest {
    period: FinancialPeriod,
    memo: Option<String>,
    entries: Vec<EntryRequest>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum EntryRequest {
    Account {
        account_id: String,
        amount: String,
        entry_side: EntrySide,
    },
    Activity {
        activity_id: String,
        fund_id: String,
        #[serde(default)]
        transfer: bool,
        amount: String,
        entry_side: EntrySide,
    },
}

fn parse_amount(amount: &str) -> Result<u64, TransactionValidationError> {
    let dollars = Decimal::from_str_exact(amount)
        .map_err(|_| TransactionValidationError::ParseDecimal(amount.to_owned()))?;
    if dollars != dollars.trunc_with_scale(2) {
        return Err(TransactionValidationError::PartialCentValue(
            amount.to_owned(),
        ));
    }

    let cents = dollars
        .checked_mul(dec!(100))
        .and_then(|cents| cents.to_i64())
        .ok_or_else(|| TransactionValidationError::OutOfRange(amount.to_owned()))?;
    if cents <= 0 {
        return Err(TransactionValidationError::NegativeEntryAmount(
            amount.to_owned(),
        ));
    }

    Ok(cents.unsigned_abs())
}

pub async fn transact(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    request: Result<Json<TransactRequest>, JsonRejection>,
) -> Result<Redirect, JournalError> {
    let journal_id = JournalId::from_str(&id)?;
    let user = get_user(session)?;
    let authority = Authority::Direct(Actor::User(user.id));
    let Json(request) =
        request.map_err(|error| TransactionValidationError::InvalidJson(error.body_text()))?;

    if request.entries.is_empty() {
        return Err(TransactionValidationError::NoTransactionEntries.into());
    }

    let mut entries = Vec::with_capacity(request.entries.len());

    for entry in request.entries {
        let (entry_kind, amount, entry_side) = match entry {
            EntryRequest::Account {
                account_id,
                amount,
                entry_side,
            } => (
                EntryKind::Account {
                    account_id: account_id.parse()?,
                },
                amount,
                entry_side,
            ),
            EntryRequest::Activity {
                activity_id,
                fund_id,
                transfer,
                amount,
                entry_side,
            } => (
                EntryKind::Activity {
                    activity_id: activity_id.parse()?,
                    fund_id: fund_id.parse()?,
                    scope: if transfer {
                        ActivityScope::Internal
                    } else {
                        ActivityScope::External
                    },
                },
                amount,
                entry_side,
            ),
        };

        entries.push(TransactionEntry {
            entry_kind,
            amount: parse_amount(&amount)?,
            entry_side,
        });
    }

    let memo = request
        .memo
        .map(Memo::try_new)
        .transpose()
        .map_err(TransactionValidationError::Memo)?;

    let event_id = state
        .journal_service
        .create_transaction(
            TransactionId::new(),
            journal_id,
            entries,
            request.period,
            memo,
            authority,
            DefaultTimeProvider.get_time(),
        )
        .await?;

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(&format!("/journal/{journal_id}/transaction")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exact_cents_at_the_storage_limit() {
        assert_eq!(parse_amount("92233720368547758.07"), Ok(i64::MAX as u64));
        assert_eq!(
            parse_amount("92233720368547758.071"),
            Err(TransactionValidationError::PartialCentValue(
                "92233720368547758.071".into()
            ))
        );
        assert_eq!(
            parse_amount("92233720368547758.08"),
            Err(TransactionValidationError::OutOfRange(
                "92233720368547758.08".into()
            ))
        );
    }
}
