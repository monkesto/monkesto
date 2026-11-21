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
    let create = html! {
        section id="create" aria-labelledby="create-heading" class="scroll-mt-6 rounded-xl border border-gray-200 bg-white p-5 sm:p-6 dark:border-gray-700 dark:bg-gray-800" {
            div class="mb-5" {
                h2 id="create-heading" class="text-lg font-semibold text-gray-900 dark:text-white" { "Add donor" }
                p class="mt-1 text-sm text-gray-500 dark:text-gray-400" { "Add a new donor to this journal." }
            }
            form method="post" action=(format!("/journal/{journal_id}/createdonor")) class="flex max-w-2xl flex-col gap-5" {
                div {
                    label for="donor_name" class="mb-1.5 block text-sm font-medium text-gray-700 dark:text-gray-200" { "Name" }
                    input type="text" id="donor_name" name="donor_name" required maxlength="64" placeholder="donor name" class=(ui::INPUT);
                }

                div { button type="submit" class=(ui::BUTTON) { "Add donor" } }
            }
        }
    };

    let content = ui::directory(
        journal_id,
        &query,
        entries,
        Directory {
            section: "donor",
            title: "Donors",
            singular: "donor",
            description: "Find donors and review their total contributions to this journal.",
            button: "Add donor",
            show_kind: false,
            amount_label: "Total donated",
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
