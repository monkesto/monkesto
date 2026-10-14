use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::journal::JournalId;
use crate::journal::error::JournalError;
use crate::journal::file::FileId;
use crate::{BackendType, StateType};
use axum::extract::{Path, State};
use axum::response::Redirect;
use axum_login::AuthSession;
use axum_login::tracing::info;
use std::str::FromStr;
use std::time::Instant;

pub async fn import_jewel_db(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((journal_id, file_id)): Path<(String, String)>,
) -> Result<Redirect, JournalError> {
    let user = get_user(session)?;
    let user_authority = Authority::Direct(Actor::User(user.id));
    let journal_id = JournalId::from_str(&journal_id)?;
    let file_id = FileId::from_str(&file_id)?;

    let data_gather_start = Instant::now();

    let jewel_data = state
        .journal_service
        .get_jewel_db(journal_id, file_id, user_authority, 0, 0)
        .await?;

    info!(
        "Jewel database imported in {:?})",
        data_gather_start.elapsed()
    );

    let num_accounts = jewel_data.accounts.len();
    let num_transactions = jewel_data.journals.len();

    let event_id = state
        .journal_service
        .import_jewel_database(journal_id, jewel_data, user_authority)
        .await?;

    info!(
        "Imported {} accounts and {} transaction in {:?}",
        num_accounts,
        num_transactions,
        data_gather_start.elapsed()
    );

    let projection_update_start = Instant::now();

    state.journal_service.wait_for(event_id).await;

    info!(
        "projection updated in {:?} ({:?} total)",
        projection_update_start.elapsed(),
        data_gather_start.elapsed()
    );

    Ok(Redirect::to(&format!("/journal/{}", journal_id)))
}
