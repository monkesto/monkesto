use super::JournalEvent;
use crate::AppState;
use crate::auth::UserStore;
use crate::auth::axum_login::AuthSession;
use crate::auth::{self, user};
use crate::cuid::Cuid;
use crate::journal::JournalStore;
use crate::journal::JournalTenantInfo;
use crate::journal::Permissions;
use crate::known_errors::{KnownErrors, RedirectOnError};
use crate::webauthn::user::Email;
use auth::user::UserEvent;
use axum::Form;
use axum::extract::Path;
use axum::extract::State;
use axum::response::Redirect;
use chrono::Utc;
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

    let user_id = user::get_id(session)?;

    if form.journal_name.trim().is_empty() {
        return Err(KnownErrors::InvalidInput.redirect("/journal"));
    }

    let journal_id = Cuid::new10();

    state
        .journal_store
        .create_journal(JournalEvent::Created {
            id: journal_id,
            name: form.journal_name,
            creator: user_id,
            created_at: Utc::now(),
        })
        .await
        .or_redirect(CALLBACK_URL)?;

    state
        .user_store
        .push_event(&user_id, UserEvent::CreatedJournal { journal_id })
        .await
        .or_redirect(CALLBACK_URL)?;

    Ok(Redirect::to(CALLBACK_URL))
}

#[derive(Deserialize)]
pub struct InviteUserForm {
    email: Email,
}

pub async fn invite_user(
    State(state): State<AppState>,
    session: AuthSession,
    Path(id): Path<String>,
    Form(form): Form<InviteUserForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/person", id);

    let user_id = user::get_id(session)?;

    let journal_id = Cuid::from_str(&id).or_redirect(callback_url)?;

    let journal_state = state
        .journal_store
        .get_journal(&journal_id)
        .await
        .or_redirect(callback_url)?;

    if !journal_state.deleted {
        if journal_state
            .get_user_permissions(&user_id)
            .contains(Permissions::INVITE)
        {
            // TODO: add a selector for permissions
            let invitee_permissions = Permissions::all();

            let tenant_info = JournalTenantInfo {
                tenant_permissions: invitee_permissions,
                inviting_user: user_id,
                invited_at: Utc::now(),
            };

            let invitee_id = state
                .user_store
                .lookup_user_id(&form.email)
                .await
                .or_redirect(callback_url)?
                .ok_or(KnownErrors::UserDoesntExist)
                .or_redirect(callback_url)?;

            if !journal_state.get_user_permissions(&invitee_id).is_empty() {
                return Err(KnownErrors::UserCanAccessJournal.redirect(callback_url));
            }

            state
                .journal_store
                .push_event(
                    &journal_id,
                    JournalEvent::AddedTenant {
                        id: invitee_id,
                        tenant_info,
                    },
                )
                .await
                .or_redirect(callback_url)?;

            state
                .user_store
                .push_event(&invitee_id, UserEvent::InvitedToJournal { journal_id })
                .await
                .or_redirect(callback_url)?;

            //TODO: add a menu for the user to accept or decline the invitation
            state
                .user_store
                .push_event(&invitee_id, UserEvent::AcceptedJournalInvite { journal_id })
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

    let journal_id = Cuid::from_str(&id).or_redirect(callback_url)?;

    let user_id = user::get_id(session)?;

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
            .get_user_permissions(&user_id)
            .contains(Permissions::ADDACCOUNT)
        {
            state
                .journal_store
                .push_event(
                    &journal_id,
                    JournalEvent::CreatedAccount {
                        id: Cuid::new10(),
                        name: form.account_name,
                        created_by: user_id,
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

/*
These functions are unused. They are kept to serve as a guide
for future implementations.

pub async fn respond_to_journal_invite(
    journal_id: String,
    accepted: String,
) -> Result<(), ServerFnError> {
    let journal_id = Cuid::from_str(&journal_id)?;

    let accepted: bool = serde_json::from_str(&accepted)?;

    use UserEventType::*;

    let session_id = extensions::get_session_id().await?;
    let pool = extensions::get_pool().await?;

    let user_id = auth::get_user_id(&session_id, &pool).await?;

    let user_state = UserState::build(
        &user_id,
        vec![
            InvitedToJournal,
            AcceptedJournalInvite,
            DeclinedJournalInvite,
            RemovedFromJournal,
        ],
        &pool,
    )
    .await?;

    if user_state.pending_journal_invites.contains_key(&journal_id) {
        if accepted {
            UserEvent::AcceptedJournalInvite { id: journal_id }
                .push_db(&user_id, &pool)
                .await?;
        } else {
            UserEvent::DeclinedJournalInvite { id: journal_id }
                .push_db(&user_id, &pool)
                .await?;
        }
    } else {
        return Err(ServerFnError::ServerError(
            KnownErrors::NoInvitation.to_string()?,
        ));
    }

    Ok(())
}

pub async fn add_account(journal_id: Cuid, account_name: String) -> Result<(), ServerFnError> {
    use UserEventType::*;

    let session_id = extensions::get_session_id().await?;
    let pool = extensions::get_pool().await?;

    let user_id = auth::get_user_id(&session_id, &pool).await?;

    if account_name.trim().is_empty() {
        return Err(ServerFnError::ServerError(
            KnownErrors::InvalidInput.to_string()?,
        ));
    }

    let user_state = UserState::build(
        &user_id,
        vec![
            CreatedJournal,
            InvitedToJournal,
            AcceptedJournalInvite,
            DeclinedJournalInvite,
            RemovedFromJournal,
        ],
        &pool,
    )
    .await?;

    if user_state.owned_journals.contains(&journal_id)
        || user_state
            .accepted_journal_invites
            .get(&journal_id)
            .is_some_and(|tenant_info| {
                tenant_info
                    .tenant_permissions
                    .contains(Permissions::ADDACCOUNT)
            })
    {
        JournalEvent::CreatedAccount { account_name }
            .push_db(&journal_id, &pool)
            .await?;
    } else {
        return Err(ServerFnError::ServerError(
            KnownErrors::PermissionError {
                required_permissions: Permissions::ADDACCOUNT,
            }
            .to_string()?,
        ));
    }

    Ok(())
}

pub async fn transact(
    journal_id: String,
    account_ids: Vec<String>,
    balance_add_cents: Vec<String>,
    balance_remove_cents: Vec<String>,
) -> Result<(), ServerFnError> {
    use UserEventType::*;
    let journal_id = Cuid::from_str(&journal_id)?;

    let session_id = extensions::get_session_id().await?;
    let pool = extensions::get_pool().await?;

    let user_id = auth::get_user_id(&session_id, &pool).await?;

    let user_state = UserState::build(
        &user_id,
        vec![
            CreatedJournal,
            InvitedToJournal,
            AcceptedJournalInvite,
            DeclinedJournalInvite,
            RemovedFromJournal,
        ],
        &pool,
    )
    .await?;

    if !user_state.owned_journals.contains(&journal_id) {
        if let Some(tenant_info) = user_state.accepted_journal_invites.get(&journal_id) {
            if !tenant_info
                .tenant_permissions
                .contains(Permissions::APPENDTRANSACTION)
            {
                return Err(ServerFnError::ServerError(
                    KnownErrors::PermissionError {
                        required_permissions: Permissions::APPENDTRANSACTION,
                    }
                    .to_string()?,
                ));
            }
        } else {
            return Err(ServerFnError::ServerError(
                KnownErrors::PermissionError {
                    required_permissions: Permissions::APPENDTRANSACTION,
                }
                .to_string()?,
            ));
        }
    }

    let mut updates: Vec<BalanceUpdate> = Vec::new();
    let mut total_balance_change: i64 = 0;

    for i in 0..balance_add_cents.len() {
        let add_amt = (balance_add_cents[i].parse::<f64>().unwrap_or(0.0) * 100.0) as i64;

        let remove_amt = (balance_remove_cents[i].parse::<f64>().unwrap_or(0.0) * 100.0) as i64;

        let account_sum = add_amt - remove_amt;

        if account_sum != 0 {
            total_balance_change += account_sum;
            updates.push(BalanceUpdate {
                account_id: Cuid::from_str(&account_ids[i])?,
                changed_by: account_sum,
            });
        }
    }

    if total_balance_change != 0 {
        return Err(ServerFnError::ServerError(
            KnownErrors::BalanceMismatch {
                attempted_transaction: updates,
            }
            .to_string()?,
        ));
    }

    if updates.is_empty() {
        return Err(ServerFnError::ServerError(
            KnownErrors::InvalidInput.to_string()?,
        ));
    }

    JournalEvent::AddedEntry {
        transaction: Transaction {
            author: user_id,
            updates,
        },
    }
    .push_db(&journal_id, &pool)
    .await?;

    Ok(())
}
*/
