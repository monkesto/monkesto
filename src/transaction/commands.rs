use crate::BackendType;
use crate::StateType;
use crate::auth::user::{self};
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::known_errors::KnownErrors;
use crate::known_errors::RedirectOnError;
use crate::service::Service;
use crate::transaction::BalanceUpdate;
use crate::transaction::EntryType;
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

    let user = user::get_user(session)?;

    let mut total_change = 0;
    let mut updates = Vec::new();

    for (idx, acc_id_str) in form.account.iter().enumerate() {
        // if the id isn't valid, assume that the user just didn't select an account
        if let Ok(acc_id) = AccountId::from_str(acc_id_str) {
            // if the id doesn't map to an account, return an error

            let dec_amt = Decimal::from_str(
                form.amount
                    .get(idx)
                    .ok_or(KnownErrors::InvalidInput)
                    .or_redirect(callback_url)?,
            )
            .or_redirect(callback_url)?
                * dec!(100);

            // this will reject inputs with partial cent values
            // this should not be possible unless a user uses the
            //  inspector tool to change their HTML
            if !dec_amt.is_integer() {
                return Err(KnownErrors::InvalidInput.redirect(callback_url));
            } else {
                let amt = dec_amt
                    .to_i64()
                    .ok_or(KnownErrors::InvalidInput)
                    .or_redirect(callback_url)?;

                // error when the amount is below zero to prevent confusion with the credit/debit selector
                if amt <= 0 {
                    return Err(KnownErrors::InvalidInput).or_redirect(callback_url);
                }

                let entry_type = EntryType::from_str(
                    form.entry_type
                        .get(idx)
                        .ok_or(KnownErrors::InvalidInput)
                        .or_redirect(callback_url)?,
                )
                .or_redirect(callback_url)?;

                updates.push(BalanceUpdate {
                    account_id: acc_id,
                    amount: amt as u64,
                    entry_type,
                });

                total_change += amt
                    * if entry_type == EntryType::Credit {
                        1
                    } else {
                        -1
                    };
            }
        }
    }

    // if total change isn't zero, return an error
    if total_change != 0 {
        Err(KnownErrors::BalanceMismatch {
            attempted_transaction: updates,
        })
        .or_redirect(callback_url)
    } else if updates.is_empty() {
        Err(KnownErrors::InvalidInput).or_redirect(callback_url)
    } else {
        state
            .transaction_create(TransactionId::new(), journal_id, user.id, updates)
            .await
            .or_redirect(callback_url)?;

        Ok(Redirect::to(callback_url))
    }
}
