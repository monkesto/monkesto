use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::dollars::Dollars;
use crate::journal::JournalId;
use crate::journal::activity::ActivityKind;
use crate::journal::error::JournalError;
use crate::journal::layout::layout;
use crate::journal::ui::{self, Directory, DirectoryEntry, ListQuery};
use crate::{BackendType, StateType};
use axum::extract::{Path, Query, State};
use axum_login::AuthSession;
use maud::{Markup, html};
use std::str::FromStr;

pub async fn activity_list_page(
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
        .list_journal_activities(journal_id, authority)
        .await?;
    let entries = items
        .into_iter()
        .map(|(item, _, created)| DirectoryEntry {
            id: item.id.to_string(),
            name: item.name.to_string(),
            created,
            kind: Some(match item.activity_type {
                ActivityKind::Income => "Income",
                ActivityKind::Expense => "Expense",
            }),
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
                h2 id="create-heading" class="text-lg font-semibold text-gray-900 dark:text-white" { "Add activity" }
                p class="mt-1 text-sm text-gray-500 dark:text-gray-400" { "Add a new activity to this journal." }
            }
            form method="post" action=(format!("/journal/{journal_id}/createactivity")) class="flex max-w-2xl flex-col gap-5" {
                div {
                    label for="activity_name" class="mb-1.5 block text-sm font-medium text-gray-700 dark:text-gray-200" { "Name" }
                    input type="text" id="activity_name" name="activity_name" required maxlength="64" placeholder="activity name" class=(ui::INPUT);
                }
                fieldset {
                    legend class="mb-2 text-sm font-medium text-gray-700 dark:text-gray-200" { "Type" }
                    div class="flex flex-wrap gap-3" {
                        @for (index, (label, value)) in [("Income", ActivityKind::Income as i8), ("Expense", ActivityKind::Expense as i8)].iter().enumerate() {
                            label class="flex cursor-pointer items-center gap-2 rounded-lg border border-gray-200 px-4 py-3 text-sm text-gray-700 has-checked:border-indigo-500 has-checked:bg-indigo-50 dark:border-gray-600 dark:text-gray-200 dark:has-checked:bg-indigo-500/10" {
                                input type="radio" name="activity_kind" value=(value) required checked[index == 0] class="accent-indigo-600";
                                (label)
                            }
                        }
                    }
                }
                div { button type="submit" class=(ui::BUTTON) { "Add activity" } }
            }
        }
    };

    let content = ui::directory(
        journal_id,
        &query,
        entries,
        Directory {
            section: "activity",
            title: "Activities",
            singular: "activity",
            description: "Track income and expense activities and their current balances.",
            button: "Add activity",
            show_kind: true,
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
