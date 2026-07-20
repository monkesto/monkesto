use crate::BackendType;
use crate::StateType;
use crate::authn::get_user;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::journal::account::AccountId;
use crate::journal::transaction::{BalanceUpdate, TransactionId};
use crate::journal::transaction::{EntryType, TransactionValidationError};
use crate::journal::{JournalError, JournalId};
use crate::monkesto_error::OrRedirect;
use crate::time_provider::{DefaultTimeProvider, TimeProvider};
use axum::extract::Path;
use axum::extract::State;
use axum::response::Redirect;
use axum_extra::extract::Form;
use axum_login::AuthSession;
use rust_decimal::dec;
use rust_decimal::prelude::*;
use serde::Deserialize;
use std::str::FromStr;

#[derive(Deserialize)]
pub struct TransactForm {
    account: Vec<String>,
    amount: Vec<String>,
    entry_type: Vec<String>,
}

pub async fn transact(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Form(form): Form<TransactForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/transaction", id);

    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;

    let user = get_user(session)?;
    let user_authority = Authority::Direct(Actor::User(user.id));

    let mut updates = Vec::new();

    if form.account.is_empty() {
        return Err(JournalError::TransactionValidation(
            TransactionValidationError::NoTransactionEntries,
        ))
        .or_redirect(callback_url);
    }

    for (idx, acc_id_str) in form.account.iter().enumerate() {
        // if the id isn't valid, assume that the user just didn't select an account
        if let Ok(acc_id) = AccountId::from_str(acc_id_str) {
            let str_decimal_amt = form
                .amount
                .get(idx)
                .ok_or(JournalError::TransactionValidation(
                    TransactionValidationError::MissingEntryAmount,
                ))
                .or_redirect(callback_url)?;

            let dec_amt = Decimal::from_str(str_decimal_amt)
                .map_err(|_| {
                    JournalError::TransactionValidation(TransactionValidationError::ParseDecimal(
                        str_decimal_amt.to_string(),
                    ))
                })
                .or_redirect(callback_url)?
                * dec!(100);

            // this will reject inputs with partial cent values
            // this should not be possible unless a user uses the
            //  inspector tool to change their HTML
            if !dec_amt.is_integer() {
                return Err(JournalError::TransactionValidation(
                    TransactionValidationError::PartialCentValue(str_decimal_amt.to_string()),
                ))
                .or_redirect(callback_url);
            } else {
                let amt = dec_amt
                    .to_i64()
                    .ok_or_else(|| {
                        JournalError::TransactionValidation(TransactionValidationError::OutOfRange(
                            str_decimal_amt.to_string(),
                        ))
                    })
                    .or_redirect(callback_url)?;

                // error when the amount is below zero to prevent confusion with the credit/debit selector
                if amt <= 0 {
                    return Err(JournalError::TransactionValidation(
                        TransactionValidationError::NegativeEntryAmount(dec_amt.to_string()),
                    ))
                    .or_redirect(callback_url);
                }

                let entry_type = EntryType::from_str(
                    form.entry_type
                        .get(idx)
                        .ok_or(JournalError::TransactionValidation(
                            TransactionValidationError::MissingEntryType,
                        ))
                        .or_redirect(callback_url)?,
                )
                .or_redirect(callback_url)?;

                updates.push(BalanceUpdate {
                    account_id: acc_id,
                    amount: amt as u64,
                    entry_type,
                });
            }
        }
    }

    let event_id = state
        .journal_service
        .create_transaction(
            TransactionId::new(),
            journal_id,
            updates,
            user_authority,
            DefaultTimeProvider.get_time(),
        )
        .await
        .or_redirect(callback_url)?;

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(callback_url))
}
