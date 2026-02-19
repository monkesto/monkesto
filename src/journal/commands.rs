use crate::appstate::AppState;
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::journal::Permissions;
use crate::journal::transaction::BalanceUpdate;
use crate::journal::transaction::EntryType;
use axum_login::AuthSession;

use crate::BackendType;
use crate::StateType;
use crate::auth::user::Email;
use crate::auth::user::{self};
use crate::authority::UserId;
use crate::known_errors::KnownErrors;
use crate::known_errors::RedirectOnError;
use axum::extract::Path;
use axum::extract::State;
use axum::response::Redirect;
use axum_extra::extract::Form;
use rust_decimal::dec;
use rust_decimal::prelude::*;
use serde::Deserialize;
use std::str::FromStr;

#[derive(Deserialize)]
pub struct CreateJournalForm {
    journal_name: String,
}
pub async fn create_journal(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Form(form): Form<CreateJournalForm>,
) -> Result<Redirect, Redirect> {
    const CALLBACK_URL: &str = "/journal";

    let user = user::get_user(session)?;

    if form.journal_name.trim().is_empty() {
        return Err(KnownErrors::InvalidInput.redirect("/journal"));
    }

    state
        .journal_create(JournalId::new(), form.journal_name, user.id)
        .await
        .or_redirect(CALLBACK_URL)?;

    Ok(Redirect::to(CALLBACK_URL))
}

#[derive(Deserialize)]
pub struct InviteUserForm {
    email: String,
    pub read: Option<String>,
    pub addaccount: Option<String>,
    pub appendtransaction: Option<String>,
    pub invite: Option<String>,
    pub delete: Option<String>,
}

pub async fn invite_user(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Form(form): Form<InviteUserForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/person", id);

    let email = Email::try_new(form.email)
        .map_err(|_| KnownErrors::UserDoesntExist)
        .or_redirect(callback_url)?;

    let user = user::get_user(session)?;

    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;

    let mut invitee_permissions = Permissions::empty();
    if form.read.is_some() {
        invitee_permissions.insert(Permissions::READ);
    }
    if form.addaccount.is_some() {
        invitee_permissions.insert(Permissions::ADDACCOUNT);
    }
    if form.appendtransaction.is_some() {
        invitee_permissions.insert(Permissions::APPENDTRANSACTION);
    }
    if form.invite.is_some() {
        invitee_permissions.insert(Permissions::INVITE);
    }
    if form.delete.is_some() {
        invitee_permissions.insert(Permissions::DELETE);
    }

    state
        .journal_invite_tenant(journal_id, user.id, email, invitee_permissions)
        .await
        .or_redirect(callback_url)?;

    Ok(Redirect::to(callback_url))
}

#[derive(Deserialize)]
pub struct CreateAccountForm {
    account_name: String,
}

pub async fn create_account(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Form(form): Form<CreateAccountForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/account", id);

    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;

    let user = user::get_user(session)?;

    if form.account_name.trim().is_empty() || form.account_name.len() > 64 {
        return Err(KnownErrors::InvalidInput).or_redirect(callback_url)?;
    }

    state
        .account_create(AccountId::new(), journal_id, user.id, form.account_name)
        .await
        .or_redirect(callback_url)?;

    Ok(Redirect::to(callback_url))
}

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

#[derive(Deserialize)]
pub struct UpdatePermissionsForm {
    pub read: Option<String>,
    pub addaccount: Option<String>,
    pub appendtransaction: Option<String>,
    pub invite: Option<String>,
    pub delete: Option<String>,
}

pub async fn update_permissions(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((id, person_id)): Path<(String, String)>,
    Form(form): Form<UpdatePermissionsForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/person/{}", id, person_id);

    let user = user::get_user(session)?;
    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;
    let target_user_id = UserId::from_str(&person_id).or_redirect(callback_url)?;

    let mut new_permissions = Permissions::empty();
    if form.read.is_some() {
        new_permissions.insert(Permissions::READ);
    }
    if form.addaccount.is_some() {
        new_permissions.insert(Permissions::ADDACCOUNT);
    }
    if form.appendtransaction.is_some() {
        new_permissions.insert(Permissions::APPENDTRANSACTION);
    }
    if form.invite.is_some() {
        new_permissions.insert(Permissions::INVITE);
    }
    if form.delete.is_some() {
        new_permissions.insert(Permissions::DELETE);
    }

    state
        .journal_update_tenant_permissions(journal_id, target_user_id, new_permissions, user.id)
        .await
        .or_redirect(callback_url)?;

    Ok(Redirect::to(callback_url))
}

pub async fn remove_tenant(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((id, person_id)): Path<(String, String)>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/person", id);
    let person_detail_url = &format!("/journal/{}/person/{}", id, person_id);

    let user = user::get_user(session)?;
    let journal_id = JournalId::from_str(&id).or_redirect(person_detail_url)?;
    let target_user_id = UserId::from_str(&person_id).or_redirect(person_detail_url)?;

    state
        .journal_remove_tenant(journal_id, target_user_id, user.id)
        .await
        .or_redirect(callback_url)?;

    Ok(Redirect::to(callback_url))
}
