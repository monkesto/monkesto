use super::{BalanceUpdate, JournalEvent, Permissions, Transaction};
use crate::auth;
use crate::cuid::Cuid;
use crate::extensions;
use crate::known_errors::{KnownErrors, RedirectOnError};
use auth::user::{UserEvent, UserEventType, UserState};
use auth::username;
use axum::Extension;
use axum::Form;
use axum::response::Redirect;
use leptos::prelude::{ServerFnError, server};
use serde::Deserialize;
use sqlx::PgPool;
use tower_sessions::Session;

#[derive(Deserialize)]
pub struct CreateJournalForm {
    journal_name: String,
}

pub async fn create_journal(
    Extension(pool): Extension<PgPool>,
    session: Session,
    Form(form): Form<CreateJournalForm>,
) -> Result<Redirect, Redirect> {
    let session_id = extensions::intialize_session(&session)
        .await
        .or_redirect(KnownErrors::SessionIdNotFound, "/login")?;

    let user_id = auth::get_user_id(&session_id, &pool)
        .await
        .or_redirect(KnownErrors::NotLoggedIn, "/login")?;

    if form.journal_name.trim().is_empty() {
        return Err(KnownErrors::InvalidInput.redirect("/journal"));
    }

    let journal_id = Cuid::new10();

    JournalEvent::Created {
        name: form.journal_name,
        owner: user_id,
    }
    .push_db(&journal_id, &pool)
    .await
    .or_redirect(
        KnownErrors::InternalError {
            context: "pushing journal creation".to_string(),
        },
        "/journal",
    )?;

    UserEvent::CreatedJournal { id: journal_id }
        .push_db(&user_id, &pool)
        .await
        .or_redirect(
            KnownErrors::InternalError {
                context: "pushing user_journal creation".to_string(),
            },
            "/journal",
        )?;

    Ok(Redirect::to("/journal"))
}

#[server]
pub async fn select_journal(journal_id: String) -> Result<(), ServerFnError> {
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
        let journal = user_state.accepted_journal_invites.get(&journal_id);

        if !journal.is_some_and(|j| j.tenant_permissions.contains(Permissions::READ)) {
            return Err(ServerFnError::ServerError(
                KnownErrors::PermissionError {
                    required_permissions: Permissions::READ,
                }
                .to_string()?,
            ));
        }
    }

    UserEvent::SelectedJournal { id: journal_id }
        .push_db(&user_id, &pool)
        .await?;

    Ok(())
}

#[server]
pub async fn invite_to_journal(
    journal_id: String,
    invitee_username: String,
    permissions: String,
) -> Result<(), ServerFnError> {
    use UserEventType::*;

    let journal_id = Cuid::from_str(&journal_id)?;
    let permissions: Permissions = serde_json::from_str(&permissions)?;

    let session_id = extensions::get_session_id().await?;
    let pool = extensions::get_pool().await?;

    let own_id = auth::get_user_id(&session_id, &pool).await?;

    if let Some(invitee_id) = username::get_id(&invitee_username, &pool).await? {
        let inviting_user_state = UserState::build(
            &own_id,
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

        let invitee_state = UserState::build(
            &invitee_id,
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

        if invitee_state.owned_journals.contains(&journal_id)
            || invitee_state
                .accepted_journal_invites
                .contains_key(&journal_id)
            || invitee_state
                .pending_journal_invites
                .contains_key(&journal_id)
        {
            return Err(ServerFnError::ServerError(
                KnownErrors::UserCanAccessJournal.to_string()?,
            ));
        }

        if inviting_user_state.owned_journals.contains(&journal_id) {
            UserEvent::InvitedToJournal {
                id: journal_id,
                permissions,
                inviting_user: own_id,
                owner: own_id,
            }
            .push_db(&invitee_id, &pool)
            .await?;
        } else if let Some(own_tenant_info) = inviting_user_state
            .accepted_journal_invites
            .get(&journal_id)
        {
            for permission in permissions {
                if !own_tenant_info.tenant_permissions.contains(permission) {
                    return Err(ServerFnError::ServerError(
                        KnownErrors::PermissionError {
                            required_permissions: permission,
                        }
                        .to_string()?,
                    ));
                }
            }
            UserEvent::InvitedToJournal {
                id: journal_id,
                permissions,
                inviting_user: own_id,
                owner: own_tenant_info.journal_owner,
            }
            .push_db(&invitee_id, &pool)
            .await?;
        }
        Ok(())
    } else {
        Err(ServerFnError::ServerError(
            KnownErrors::UserDoesntExist.to_string()?,
        ))
    }
}

#[server]
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

#[server]
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

#[server]
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
