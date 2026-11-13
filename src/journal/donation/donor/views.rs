use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::dollars::Dollars;
use crate::journal::JournalId;
use crate::journal::error::JournalError;
use crate::journal::layout::layout;
use crate::journal::ui::{self, Directory, DirectoryEntry, ListQuery};
use crate::{BackendType, StateType};
use axum::extract::{Path, Query, State};
use axum_login::AuthSession;
use maud::Markup;
use std::str::FromStr;

pub async fn donor_list_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Markup, JournalError> {
    let user = get_user(session)?;
    let authority = Authority::Direct(Actor::User(user.id));
    let journal_id = JournalId::from_str(&id)?;
    let items = state
        .journal_service
        .list_journal_donors(journal_id, authority)
        .await?;
    let entries = items
        .into_iter()
        .map(|(item, _, created)| DirectoryEntry {
            id: item.id.to_string(),
            name: item.name.to_string(),
            created,
            kind: None,
            amount: Dollars(item.total_donated as i64),
            side: None,
        })
        .collect();
    let (journal, _, _) = state
        .journal_service
        .get_journal(journal_id, authority)
        .await?;
    let content = ui::directory(
        journal_id,
        &query,
        entries,
        Directory {
            section: "donor",
            title: "Donors",
            singular: "donor",
            description: "Find donors and review their total contributions to this journal.",
            action: "createdonor",
            button: "Add donor",
            types: &[],
            type_field: "",
        },
    );
    Ok(layout(
        Some(journal.name.as_ref()),
        true,
        Some(&id),
        content,
    ))
}
