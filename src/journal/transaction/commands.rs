use crate::BackendType;
use crate::StateType;
use crate::authn::get_user;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::event_id::GetEventId;
use crate::journal::JournalId;
use crate::journal::account::AccountId;
use crate::journal::transaction::TransactionError::InvalidBalanceUpdates;
use crate::journal::transaction::TransactionError::ParseDecimal;
use crate::journal::transaction::{BalanceUpdate, TransactionId};
use crate::journal::transaction::{CreateTransaction, EntryType};
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
        return Err(InvalidBalanceUpdates(
            "no balance updates were supplied".to_string(),
        ))
        .or_redirect(callback_url);
    }

    for (idx, acc_id_str) in form.account.iter().enumerate() {
        // if the id isn't valid, assume that the user just didn't select an account
        if let Ok(acc_id) = AccountId::from_str(acc_id_str) {
            let str_decimal_amt = form
                .amount
                .get(idx)
                .ok_or(InvalidBalanceUpdates(
                    "no entries were provided".to_string(),
                ))
                .or_redirect(callback_url)?;

            let dec_amt = Decimal::from_str(str_decimal_amt)
                .map_err(|_| ParseDecimal(str_decimal_amt.to_string()))
                .or_redirect(callback_url)?
                * dec!(100);

            // this will reject inputs with partial cent values
            // this should not be possible unless a user uses the
            //  inspector tool to change their HTML
            if !dec_amt.is_integer() {
                return Err(InvalidBalanceUpdates(
                    "at least one entry contained a partial cent amount value".to_string(),
                ))
                .or_redirect(callback_url);
            } else {
                let amt = dec_amt
                    .to_i64()
                    .ok_or(InvalidBalanceUpdates(
                        "at least one entry was out of range for a 64-bit integer".to_string(),
                    ))
                    .or_redirect(callback_url)?;

                // error when the amount is below zero to prevent confusion with the credit/debit selector
                if amt <= 0 {
                    return Err(InvalidBalanceUpdates("at least one entry contained a negative amount, use the debit/credit selector instead".to_string()))
                        .or_redirect(callback_url);
                }

                let entry_type = EntryType::from_str(
                    form.entry_type
                        .get(idx)
                        .ok_or(InvalidBalanceUpdates(
                            "at least one entry was missing an entry type".to_string(),
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
        .decision_maker
        .make(CreateTransaction::new(
            TransactionId::new(),
            journal_id,
            updates,
            user_authority,
            DefaultTimeProvider.get_time(),
        ))
        .await
        .or_redirect(callback_url)?
        .event_id();

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(callback_url))
}
