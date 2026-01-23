use super::JournalEvent;
use crate::AppState;
use crate::AuthSession;
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::journal::Permissions;
use crate::journal::transaction::BalanceUpdate;
use crate::journal::transaction::EntryType;
use crate::journal::transaction::TransactionEvent;

use crate::journal::{JournalStore, JournalTenantInfo};
use crate::known_errors::{KnownErrors, RedirectOnError};
use crate::webauthn::user::Email;
use crate::webauthn::user::{self, UserStore};
use axum::extract::Path;
use axum::extract::State;
use axum::response::Redirect;
use axum_extra::extract::Form;
use chrono::Utc;
use rust_decimal::dec;
use rust_decimal::prelude::*;
use serde::Deserialize;
use std::str::FromStr;

#[derive(Deserialize)]
pub struct CreateJournalForm {
    journal_name: String,
}

pub async fn create_journal(
    State(state): State<AppState>,
    session: AuthSession,
    Form(form): Form<CreateJournalForm>,
) -> Result<Redirect, Redirect> {
    const CALLBACK_URL: &str = "/journal";

    let user = user::get_user(session)?;

    if form.journal_name.trim().is_empty() {
        return Err(KnownErrors::InvalidInput.redirect("/journal"));
    }

    let journal_id = JournalId::new();

    state
        .journal_store
        .create_journal(JournalEvent::Created {
            id: journal_id,
            name: form.journal_name,
            creator: user.id,
            created_at: Utc::now(),
        })
        .await
        .or_redirect(CALLBACK_URL)?;

    Ok(Redirect::to(CALLBACK_URL))
}

#[derive(Deserialize)]
pub struct InviteUserForm {
    email: String,
}

pub async fn invite_user(
    State(state): State<AppState>,
    session: AuthSession,
    Path(id): Path<String>,
    Form(form): Form<InviteUserForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/person", id);

    let email = Email::try_new(form.email)
        .map_err(|_| KnownErrors::UserDoesntExist)
        .or_redirect(callback_url)?;

    let user = user::get_user(session)?;

    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;

    let journal_state = state
        .journal_store
        .get_journal(&journal_id)
        .await
        .or_redirect(callback_url)?;

    if !journal_state.deleted {
        if journal_state
            .get_user_permissions(&user.id)
            .contains(Permissions::INVITE)
        {
            // TODO: add a selector for permissions
            let invitee_permissions = Permissions::all();

            let tenant_info = JournalTenantInfo {
                tenant_permissions: invitee_permissions,
                inviting_user: user.id,
                invited_at: Utc::now(),
            };

            let invitee_id = state
                .user_store
                .lookup_user_id(email.as_ref())
                .await
                .map_err(|e| KnownErrors::InternalError {
                    context: e.to_string(),
                })
                .or_redirect(callback_url)?
                .ok_or(KnownErrors::UserDoesntExist)
                .or_redirect(callback_url)?;

            if !journal_state.get_user_permissions(&invitee_id).is_empty() {
                return Err(KnownErrors::UserCanAccessJournal.redirect(callback_url));
            }

            state
                .journal_store
                .push_journal_event(
                    &journal_id,
                    JournalEvent::AddedTenant {
                        id: invitee_id,
                        tenant_info,
                    },
                )
                .await
                .or_redirect(callback_url)?;
        } else {
            return Err(KnownErrors::PermissionError {
                required_permissions: Permissions::INVITE,
            }
            .redirect(callback_url));
        }
    } else {
        return Err(KnownErrors::InvalidJournal.redirect(callback_url));
    }

    Ok(Redirect::to(callback_url))
}

#[derive(Deserialize)]
pub struct CreateAccountForm {
    account_name: String,
}

pub async fn create_account(
    State(state): State<AppState>,
    session: AuthSession,
    Path(id): Path<String>,
    Form(form): Form<CreateAccountForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/account", id);

    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;

    let user = user::get_user(session)?;

    if form.account_name.trim().is_empty() {
        return Err(KnownErrors::InvalidInput).or_redirect(callback_url)?;
    }

    let journal_state = state
        .journal_store
        .get_journal(&journal_id)
        .await
        .or_redirect(callback_url)?;

    if !journal_state.deleted {
        if journal_state
            .get_user_permissions(&user.id)
            .contains(Permissions::ADDACCOUNT)
        {
            state
                .journal_store
                .push_journal_event(
                    &journal_id,
                    JournalEvent::CreatedAccount {
                        id: AccountId::new(),
                        name: form.account_name,
                        created_by: user.id,
                        created_at: Utc::now(),
                    },
                )
                .await
                .or_redirect(callback_url)?;
        } else {
            return Err(KnownErrors::PermissionError {
                required_permissions: Permissions::ADDACCOUNT,
            }
            .redirect(callback_url));
        }
    } else {
        return Err(KnownErrors::InvalidJournal.redirect(callback_url));
    }

    Ok(Redirect::to(callback_url))
}

#[derive(Deserialize)]
pub struct TransactForm {
    account: Vec<String>,
    amount: Vec<String>,
    entry_type: Vec<String>,
}

pub async fn transact(
    State(state): State<AppState>,
    session: AuthSession,
    Path(id): Path<String>,
    Form(form): Form<TransactForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/transaction", id);

    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;
    let journal_state = state
        .journal_store
        .get_journal(&journal_id)
        .await
        .or_redirect(callback_url)?;

    let user = user::get_user(session)?;

    let mut total_change = 0;
    let mut updates = Vec::new();

    for (idx, acc_id_str) in form.account.iter().enumerate() {
        // if the id isn't valid, assume that the user just didn't select an account
        if let Ok(acc_id) = AccountId::from_str(acc_id_str) {
            // if the id doesn't map to an account, return an error
            journal_state
                .accounts
                .get(&acc_id)
                .ok_or(KnownErrors::AccountDoesntExist { id: acc_id })
                .or_redirect(callback_url)?;

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
        let transaction_id = TransactionId::new();
        state
            .journal_store
            .create_transaction(
                &journal_id,
                TransactionEvent::Created {
                    id: transaction_id,
                    author: user.id,
                    updates,
                    created_at: Utc::now(),
                },
            )
            .await
            .or_redirect(callback_url)?;
        state
            .journal_store
            .push_journal_event(&journal_id, JournalEvent::AddedEntry { transaction_id })
            .await
            .or_redirect(callback_url)?;

        Ok(Redirect::to(callback_url))
    }
}
