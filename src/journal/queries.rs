use std::collections::HashMap;

use super::{AssociatedJournal, JournalEventType, JournalState};
use crate::auth;
use crate::cuid::Cuid;
use crate::known_errors::KnownErrors;
use auth::user::{UserEventType, UserState};
use auth::username;
use sqlx::PgPool;
use std::str::FromStr;

pub async fn get_associated_journals(
    user_id: &Cuid,
    pool: &PgPool,
) -> Result<HashMap<Cuid, AssociatedJournal>, KnownErrors> {
    use JournalEventType::{AddedTenant, Created, Deleted};
    use UserEventType::*;
    let mut journals = HashMap::new();

    let user = UserState::build(
        user_id,
        vec![
            CreatedJournal,
            InvitedToJournal,
            AcceptedJournalInvite,
            DeclinedJournalInvite,
            RemovedFromJournal,
        ],
        pool,
    )
    .await?;

    for journal_id in user.owned_journals {
        let journal_state = JournalState::build(&journal_id, vec![Created, Deleted], pool).await?;
        if !journal_state.deleted {
            journals.insert(
                journal_id,
                AssociatedJournal::Owned {
                    name: journal_state.name,
                    created_at: journal_state.created_at,
                },
            );
        }
    }

    for journal_id in user.accepted_journal_invites {
        let journal_state =
            JournalState::build(&journal_id, vec![Created, Deleted, AddedTenant], pool).await?;
        if !journal_state.deleted {
            journals.insert(
                journal_id,
                AssociatedJournal::Shared {
                    name: journal_state.name,
                    created_at: journal_state.created_at,
                    tenant_info: *journal_state
                        .tenants
                        .get(user_id)
                        .ok_or(KnownErrors::TenantDoesntExist)?,
                },
            );
        }
    }

    Ok(journals)
}

pub async fn get_journal_owner(
    journal_id: &str,
    pool: &PgPool,
) -> Result<Option<String>, KnownErrors> {
    let journal_id = Cuid::from_str(journal_id)?;

    let journal_state =
        JournalState::build(&journal_id, vec![JournalEventType::Created], pool).await?;

    username::get_username(&journal_state.owner, pool).await
}

/*
These functions are unused. They are kept to serve as a guide for
future implementations

pub async fn get_journal_invites() -> Result<Vec<JournalInvite>, KnownError> {
    use JournalEventType::{Created, *};
    use UserEventType::*;

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

pub async fn get_accounts() -> Result<Vec<Account>, ServerFnError> {
    use JournalEventType::{Created, *};
    use UserEventType::*;

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

pub async fn get_transactions(
    journals: Result<Journals, ServerFnError>,
) -> Result<Vec<TransactionWithTimeStamp>, ServerFnError> {
    use UserEventType::*;

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
    .bind(JournalEventType::AddedEntry)
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
*/
