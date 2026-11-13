use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::dollars::Dollars;
use crate::journal::JournalId;
use crate::journal::account::AccountType;
use crate::journal::error::JournalError;
use crate::journal::layout::layout;
use crate::journal::ui::{self, Directory, DirectoryEntry, ListQuery};
use crate::{BackendType, StateType};
use axum::extract::{Path, Query, State};
use axum_login::AuthSession;
use maud::Markup;
use std::str::FromStr;

pub async fn account_list_page(
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
        .list_journal_accounts(journal_id, authority)
        .await?;
    let entries = items
        .into_iter()
        .map(|(item, _, created)| DirectoryEntry {
            id: item.id.to_string(),
            name: item.name.to_string(),
            created,
            kind: Some(match item.account_type {
                AccountType::Asset => "Asset",
                AccountType::Liability => "Liability",
            }),
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
            section: "account",
            title: "Accounts",
            singular: "account",
            description: "Review your bank and liability accounts and their current balances.",
            action: "createaccount",
            button: "Add account",
            types: &[
                ("Asset", AccountType::Asset as i8),
                ("Liability", AccountType::Liability as i8),
            ],
            type_field: "account_type",
        },
    );
    Ok(layout(
        Some(journal.name.as_ref()),
        true,
        Some(&id),
        content,
    ))
}
