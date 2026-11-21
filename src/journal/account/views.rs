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
use maud::{Markup, html};
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
    let create = html! {
        section id="create" aria-labelledby="create-heading" class="scroll-mt-6 rounded-xl border border-gray-200 bg-white p-5 sm:p-6 dark:border-gray-700 dark:bg-gray-800" {
            div class="mb-5" {
                h2 id="create-heading" class="text-lg font-semibold text-gray-900 dark:text-white" { "Add account" }
                p class="mt-1 text-sm text-gray-500 dark:text-gray-400" { "Add a new account to this journal." }
            }
            form method="post" action=(format!("/journal/{journal_id}/createaccount")) class="flex max-w-2xl flex-col gap-5" {
                div {
                    label for="account_name" class="mb-1.5 block text-sm font-medium text-gray-700 dark:text-gray-200" { "Name" }
                    input type="text" id="account_name" name="account_name" required maxlength="64" placeholder="account name" class=(ui::INPUT);
                }
                fieldset {
                    legend class="mb-2 text-sm font-medium text-gray-700 dark:text-gray-200" { "Type" }
                    div class="flex flex-wrap gap-3" {
                        @for (index, (label, value)) in [("Asset", AccountType::Asset as i8), ("Liability", AccountType::Liability as i8)].iter().enumerate() {
                            label class="flex cursor-pointer items-center gap-2 rounded-lg border border-gray-200 px-4 py-3 text-sm text-gray-700 has-checked:border-indigo-500 has-checked:bg-indigo-50 dark:border-gray-600 dark:text-gray-200 dark:has-checked:bg-indigo-500/10" {
                                input type="radio" name="account_type" value=(value) required checked[index == 0] class="accent-indigo-600";
                                (label)
                            }
                        }
                    }
                }
                div { button type="submit" class=(ui::BUTTON) { "Add account" } }
            }
        }
    };

    let content = ui::directory(
        journal_id,
        &query,
        entries,
        Directory {
            section: "account",
            title: "Accounts",
            singular: "account",
            description: "Review your bank and liability accounts and their current balances.",
            button: "Add account",
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
