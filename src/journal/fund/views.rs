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
use maud::{Markup, html};
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
    let create = html! {
        section id="create" aria-labelledby="create-heading" class="scroll-mt-6 rounded-xl border border-gray-200 bg-white p-5 sm:p-6 dark:border-gray-700 dark:bg-gray-800" {
            div class="mb-5" {
                h2 id="create-heading" class="text-lg font-semibold text-gray-900 dark:text-white" { "Add fund" }
                p class="mt-1 text-sm text-gray-500 dark:text-gray-400" { "Add a new fund to this journal." }
            }
            form method="post" action=(format!("/journal/{journal_id}/createfund")) class="flex max-w-2xl flex-col gap-5" {
                div {
                    label for="fund_name" class="mb-1.5 block text-sm font-medium text-gray-700 dark:text-gray-200" { "Name" }
                    input type="text" id="fund_name" name="fund_name" required maxlength="64" placeholder="fund name" class=(ui::INPUT);
                }

                div { button type="submit" class=(ui::BUTTON) { "Add fund" } }
            }
        }
    };

    let content = ui::directory(
        journal_id,
        &query,
        entries,
        Directory {
            section: "fund",
            title: "Funds",
            singular: "fund",
            description: "Keep designated money organized and review each fund’s balance.",
            button: "Add fund",
            show_kind: false,
            amount_label: "Balance",
        },
        create,
    );
    Ok(layout(
        Some(journal.name.as_ref()),
        true,
        Some(&id),
        content,
    ))
}
