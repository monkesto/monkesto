use super::extensions;
use super::return_types::*;
use crate::auth;
use crate::journal;
use crate::journal::{BalanceUpdate, JournalEvent, JournalState, Permissions, Transaction};
use auth::user;
use auth::user::{UserEvent, UserState};
use auth::username;
use chrono::Utc;
use journal::JournalEventType;
use leptos::prelude::*;
use postcard::from_bytes;

#[server]
pub async fn create_journal(journal_name: String) -> Result<(), ServerFnError> {
    let session_id = extensions::get_session_id().await?;
    let pool = extensions::get_pool().await?;

    let user_id = auth::get_user_id(&session_id, &pool).await?;

    if journal_name.trim().is_empty() {
        return Err(ServerFnError::ServerError(
            KnownErrors::InvalidInput.to_string()?,
        ));
    }

    let journal_id = Cuid::new10();

    JournalEvent::Created {
        name: journal_name,
        owner: user_id,
    }
    .push_db(&journal_id, &pool)
    .await?;

    UserEvent::CreatedJournal { id: journal_id }
        .push_db(&user_id, &pool)
        .await?;

    Ok(())
}

#[server]
pub async fn select_journal(journal_id: String) -> Result<(), ServerFnError> {
    use user::UserEventType::*;

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
    use user::UserEventType::*;

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
pub async fn get_journal_invites() -> Result<Vec<JournalInvite>, ServerFnError> {
    use journal::JournalEventType::{Created, *};
    use user::UserEventType::*;

    let mut invites = Vec::new();

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

    for (id, tenant_info) in user_state.pending_journal_invites {
        let journal_state = JournalState::build(&id, vec![Created, Renamed], &pool).await?;

        invites.push(JournalInvite {
            id,
            name: journal_state.name,
            tenant_info,
        })
    }

    Ok(invites)
}

#[server]
pub async fn respond_to_journal_invite(
    journal_id: String,
    accepted: String,
) -> Result<(), ServerFnError> {
    let journal_id = Cuid::from_str(&journal_id)?;

    let accepted: bool = serde_json::from_str(&accepted)?;

    use user::UserEventType::*;

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
pub async fn get_associated_journals() -> Result<Journals, ServerFnError> {
    use journal::JournalEventType::{Created, Deleted};
    use user::UserEventType::*;
    let mut journals = Vec::new();

    let mut selected: Option<AssociatedJournal> = None;

    let session_id = extensions::get_session_id().await?;
    let pool = extensions::get_pool().await?;

    let user_id = auth::get_user_id(&session_id, &pool).await?;

    let user = UserState::build(
        &user_id,
        vec![
            CreatedJournal,
            InvitedToJournal,
            AcceptedJournalInvite,
            DeclinedJournalInvite,
            RemovedFromJournal,
            SelectedJournal,
        ],
        &pool,
    )
    .await?;

    for journal_id in user.owned_journals {
        let journal_state = JournalState::build(&journal_id, vec![Created, Deleted], &pool).await?;
        if !journal_state.deleted {
            journals.push(AssociatedJournal::Owned {
                id: journal_id,
                name: journal_state.name,
                created_at: journal_state.created_at,
            });
            if journal_id == user.selected_journal {
                selected = Some(
                    journals
                        .last()
                        .expect("the value was just added, it should exist.")
                        .clone(),
                )
            }
        }
    }

    for shared_journal in user.accepted_journal_invites {
        let journal_state =
            JournalState::build(&shared_journal.0, vec![Created, Deleted], &pool).await?;
        if !journal_state.deleted {
            journals.push(AssociatedJournal::Shared {
                id: shared_journal.0,
                name: journal_state.name,
                created_at: journal_state.created_at,
                tenant_info: shared_journal.1,
            });

            if shared_journal.0 == user.selected_journal {
                selected = Some(
                    journals
                        .last()
                        .expect("the value was just added, it should exist.")
                        .clone(),
                );
            }
        }
    }

    Ok(Journals {
        associated: journals,
        selected,
    })
}

#[server]
pub async fn get_journal_owner(journal_id: String) -> Result<Option<String>, ServerFnError> {
    let journal_id = Cuid::from_str(&journal_id)?;
    let pool = extensions::get_pool().await?;

    let journal_state =
        JournalState::build(&journal_id, vec![JournalEventType::Created], &pool).await?;

    username::get_username(&journal_state.owner, &pool).await
}

#[server]
pub async fn get_accounts() -> Result<Vec<Account>, ServerFnError> {
    use journal::JournalEventType::{Created, *};
    use user::UserEventType::*;

    let mut accounts = Vec::new();

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
            SelectedJournal,
        ],
        &pool,
    )
    .await?;

    let journal_id = user_state.selected_journal;

    if journal_id.is_default() {
        return Err(ServerFnError::ServerError(
            KnownErrors::InvalidJournal.to_string()?,
        ));
    }

    if !user_state.owned_journals.contains(&journal_id) {
        let journal_perms = user_state.accepted_journal_invites.get(&journal_id);

        if !journal_perms.is_some_and(|j| j.tenant_permissions.contains(Permissions::READ)) {
            return Err(ServerFnError::ServerError(
                KnownErrors::PermissionError {
                    required_permissions: Permissions::READ,
                }
                .to_string()?,
            ));
        }
    }

    let journal_state = JournalState::build(
        &journal_id,
        vec![Created, CreatedAccount, DeletedAccount, AddedEntry],
        &pool,
    )
    .await?;

    for (id, (name, balance)) in journal_state.accounts {
        accounts.push(Account { id, name, balance });
    }

    Ok(accounts)
}

#[server]
pub async fn add_account(journal_id: Cuid, account_name: String) -> Result<(), ServerFnError> {
    use user::UserEventType::*;

    let session_id = extensions::get_session_id().await?;
    let pool = extensions::get_pool().await?;

    let user_id = auth::get_user_id(&session_id, &pool).await?;

    if account_name.trim().is_empty() {
        return Err(ServerFnError::ServerError(
            KnownErrors::InvalidInput.to_string()?,
        ));
    }

    let user_state = user::UserState::build(
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
    use user::UserEventType::*;
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

pub async fn get_transactions(
    journals: Result<Journals, ServerFnError>,
) -> Result<Vec<TransactionWithTimeStamp>, ServerFnError> {
    use user::UserEventType::*;

    let journal_id = match journals?.selected {
        Some(s) => s.get_id(),
        None => {
            return Err(ServerFnError::ServerError(
                KnownErrors::InvalidJournal.to_string()?,
            ));
        }
    };

    let mut bundled_transactions = Vec::new();

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
        let shared_journal = user_state.accepted_journal_invites.get(&journal_id);
        if !shared_journal.is_some_and(|j| j.tenant_permissions.contains(Permissions::READ)) {
            return Err(ServerFnError::ServerError(
                KnownErrors::PermissionError {
                    required_permissions: Permissions::READ,
                }
                .to_string()?,
            ));
        }
    }

    let raw_transactions = sqlx::query_as::<_, (Vec<u8>, chrono::DateTime<Utc>)>(
        r#"
        SELECT payload, created_at FROM journal_events
        WHERE journal_id = $1 AND event_type = $2
        ORDER BY created_at ASC
        "#,
    )
    .bind(journal_id.to_bytes())
    .bind(journal::JournalEventType::AddedEntry)
    .fetch_all(&pool)
    .await?;

    let transactions: Vec<(Result<JournalEvent, ServerFnError>, chrono::DateTime<Utc>)> =
        raw_transactions
            .into_iter()
            .map(|(transaction, timestamp)| {
                (
                    from_bytes::<JournalEvent>(&transaction).map_err(|_| {
                        ServerFnError::ServerError("failed to deserialize transaction".to_string())
                    }),
                    timestamp,
                )
            })
            .collect();

    for transaction in transactions {
        let event: JournalEvent = transaction.0?;
        let timestamp = transaction.1;

        if let JournalEvent::AddedEntry { transaction } = event {
            let author = username::get_username(&transaction.author, &pool)
                .await?
                .unwrap_or("unknown user".to_string());

            bundled_transactions.push(TransactionWithTimeStamp {
                transaction: TransactionWithUsername {
                    author,
                    updates: transaction.updates,
                },
                timestamp,
            })
        }
    }
    Ok(bundled_transactions)
}
