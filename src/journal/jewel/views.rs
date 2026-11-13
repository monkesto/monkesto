use super::commands::authorize_import;
use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::journal::JournalId;
use crate::journal::error::JournalError;
use crate::journal::file::FileId;
use crate::journal::layout::layout;
use crate::{BackendType, StateType};
use axum::extract::{Path, State};
use axum_login::AuthSession;
use maud::{Markup, PreEscaped, html};
use std::str::FromStr;

pub async fn import_jewel_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((journal_id, file_id)): Path<(String, String)>,
) -> Result<Markup, JournalError> {
    let user = get_user(session)?;
    let authority = Authority::Direct(Actor::User(user.id));
    let journal_id = JournalId::from_str(&journal_id)?;
    let file_id = FileId::from_str(&file_id)?;
    authorize_import(&state.journal_service, journal_id, file_id, authority).await?;
    let file = state
        .journal_service
        .get_file(file_id, journal_id, authority)
        .await?;
    let (journal, _, _) = state
        .journal_service
        .get_journal(journal_id, authority)
        .await?;
    let content = html! {
        main id="jewel-import" data-url=(format!("/journal/{journal_id}/file/{file_id}/import_jewel"))
            class="mx-auto max-w-4xl space-y-6 text-gray-900 dark:text-gray-100" {
            div {
                h1 class="text-2xl font-semibold" { "Import Jewel database" }
                p class="mt-2 text-sm text-gray-600 dark:text-gray-400" { (file.name) }
            }
            ol class="grid grid-cols-1 gap-3 text-sm sm:grid-cols-3" aria-label="Import stages" {
                li id="jewel-stage-prepare" class="rounded-md border border-gray-300 p-3 dark:border-gray-700" { "1. Read and reconcile" }
                li id="jewel-stage-save" class="rounded-md border border-gray-300 p-3 dark:border-gray-700" { "2. Save records" }
                li id="jewel-stage-project" class="rounded-md border border-gray-300 p-3 dark:border-gray-700" { "3. Update balances" }
            }
            section class="space-y-3 rounded-lg bg-white p-6 shadow-sm dark:bg-gray-800" {
                p id="jewel-status" role="status" aria-live="polite" { "Opening the import…" }
                progress id="jewel-progress" class="h-3 w-full accent-indigo-600" aria-label="Import progress" {}
                p id="jewel-counts" class="text-sm text-gray-600 dark:text-gray-400" {}
                p class="text-sm text-gray-600 dark:text-gray-400" {
                    "Large databases may need several minutes after records are saved to finish updating balances. "
                    "The import continues if you leave this page; return here to check it."
                }
            }
            div id="jewel-error" hidden role="alert" class="rounded-md bg-red-50 p-4 text-red-800 dark:bg-red-950 dark:text-red-200" {}
            section id="jewel-review" hidden class="space-y-5" {
                h2 class="text-xl font-semibold" { "Review and reconcile" }
                p id="jewel-summary" {}
                p class="text-sm text-gray-600 dark:text-gray-400" {
                    "No import changes have been saved yet. Check reversals use their deposit journal's bank and fund entries. "
                    "The original donor contributions remain in giving history. "
                    "Fund corrections move money between funds without changing the bank balance or donor total."
                }
                form id="jewel-confirm" class="space-y-5" {
                    div id="jewel-issues" class="space-y-5" {}
                    button id="jewel-confirm-button" type="submit"
                        class="rounded-md bg-indigo-600 px-5 py-2 font-medium text-white hover:bg-indigo-700 disabled:opacity-50" { "Import records" }
                }
            }
            a id="jewel-complete" hidden href=(format!("/journal/{journal_id}"))
                class="inline-block rounded-md bg-indigo-600 px-5 py-2 font-medium text-white hover:bg-indigo-700" { "Open journal" }
            button id="jewel-retry" hidden type="button"
                class="rounded-md border border-gray-300 px-5 py-2 font-medium dark:border-gray-700" { "Check status and retry" }
            noscript { "JavaScript is required to monitor this import and review discrepancies." }
        }
        script { (PreEscaped(include_str!("import.js"))) }
    };
    Ok(layout(
        Some(journal.name.as_ref()),
        true,
        Some(journal_id.as_str()),
        content,
    ))
}
