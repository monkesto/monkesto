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

pub async fn fund_list_page(
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
        .list_journal_funds(journal_id, authority)
        .await?;
    let entries = items
        .into_iter()
        .map(|(item, _, created)| DirectoryEntry {
            id: item.id.to_string(),
            name: item.name.to_string(),
            created,
            kind: None,
            amount: Dollars(item.balance),
            side: Some(if item.balance < 0 { "Dr" } else { "Cr" }),
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
            section: "fund",
            title: "Funds",
            singular: "fund",
            description: "Keep designated money organized and review each fund’s balance.",
            action: "createfund",
            button: "Add fund",
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
