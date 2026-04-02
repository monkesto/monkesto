use crate::BackendType;
use crate::StateType;
use crate::account::views::render_account_options;
use crate::auth::user;
use crate::authority::Actor;
use crate::authority::Authority;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::journal::JournalNameOrUnknown;
use crate::journal::JournalState;
use crate::journal::layout;
use crate::monkesto_error::MonkestoError;
use crate::monkesto_error::UrlError;
use crate::transaction::EntryType;
use crate::transaction::TransactionState;
use axum::extract::Path;
use axum::extract::Query;
use axum::extract::State;
use axum::response::Redirect;
use axum_login::AuthSession;
use maud::Markup;
use maud::html;
use std::str::FromStr;

/// Recursively renders `<option>` elements for a journal and all its subjournals.
/// `depth` controls the `↳ ` prefix count (0 = no prefix, 1 = one arrow, etc.).
fn render_journal_options(
    all_subjournals: &[(JournalId, JournalState)],
    parent_id: JournalId,
    parent_name: &str,
    depth: usize,
) -> Markup {
    let prefix = "↳ ".repeat(depth);
    html! {
        option value=(parent_id) { (format!("{}{}", prefix, parent_name)) }
        @for (sub_id, sub_state) in all_subjournals.iter().filter(|(_, s)| s.parent_journal_id == Some(parent_id)) {
            (render_journal_options(all_subjournals, *sub_id, &sub_state.name.to_string(), depth + 1))
        }
    }
}

use serde_json::Value;
use serde_json::json;

async fn build_transaction_node(
    transaction_id: &TransactionId,
    transaction_state: &TransactionState,
    parent_journal: JournalId,
    state: &StateType,
) -> Value {
    let mut entries_html = String::new();
    for entry in &transaction_state.updates {
        let amount = format!("${}.{:02}", entry.amount / 100, entry.amount % 100);
        let account = match state
            .account_service
            .get_full_account_path(entry.account_id)
            .await
        {
            Ok(Some(segments)) => segments
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>()
                .join(" › "),
            _ => "Unknown Account".to_string(),
        };
        entries_html.push_str(&format!(
            r#"<div class="flex justify-between items-center">
                <span class="text-sm font-medium text-white">{}</span>
                <span class="text-sm text-gray-300 whitespace-nowrap ml-4">{} {}</span>
            </div>"#,
            account, amount, entry.entry_type
        ));
    }

    let author = match state
        .transaction_service
        .get_transaction_authority(transaction_id)
        .await
    {
        Ok(auth) => match auth.actor() {
            Actor::User(id) => state
                .user_service
                .user_get_email(*id)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| "unknown user".to_string()),
            Actor::System => "system".to_string(),
            Actor::Anonymous => "anonymous".to_string(),
        },
        Err(_) => "unknown".to_string(),
    };

    let card_html = format!(
        r#"<div class="tx-card rounded-xl border border-gray-700 bg-gray-800 hover:bg-gray-700 transition-colors p-4 flex flex-col gap-1 cursor-pointer w-full box-border">
        {}
        <div class="text-xs text-gray-500 mt-1">{}</div>
    </div>"#,
        entries_html, author
    );

    json!({
        "id": transaction_id.to_string(),
        "text": card_html,
        "icon": false,
        "a_attr": { "href": format!("/journal/{}/transaction/{}", parent_journal, transaction_id), "class": "tx-link" },
        "li_attr": { "class": "tx-li" },
    })
}

async fn build_journal_tree_node(
    journal_id: JournalId,
    journal_name: &str,
    authority: &Authority,
    state: &StateType,
) -> Value {
    let mut children: Vec<Value> = Vec::new();

    if let Ok(transactions) = state
        .transaction_service
        .get_all_transactions_in_journal(journal_id, authority)
        .await
    {
        for (transaction_id, transaction_state) in transactions {
            children.push(
                build_transaction_node(&transaction_id, &transaction_state, journal_id, state)
                    .await,
            );
        }
    }

    if let Ok(subjournals) = state
        .journal_service
        .get_direct_subjournals(journal_id, authority)
        .await
    {
        for (sub_id, sub_state) in subjournals {
            children.push(
                Box::pin(build_journal_tree_node(
                    sub_id,
                    &sub_state.name.to_string(),
                    authority,
                    state,
                ))
                .await,
            );
        }
    }

    json!({
        "id": journal_id.to_string(),
        "text": journal_name,
        "children": children,
        "state": { "opened": true },
    })
}

async fn render_transactions(
    parent_journal: JournalId,
    journal_name: &str,
    authority: &Authority,
    state: &StateType,
) -> Markup {
    let root_node = build_journal_tree_node(parent_journal, journal_name, authority, state).await;
    let tree_json = json!([root_node]).to_string();

    html! {
        link rel="stylesheet"
            href="https://cdnjs.cloudflare.com/ajax/libs/jstree/3.3.16/themes/default/style.min.css" {}

        // jstree requires a regular style block
        style { (maud::PreEscaped(r#"
            #journal-tree li.tx-li,
            #journal-tree li.tx-li > a { height: auto !important; display: block !important; }
            #journal-tree li.tx-li > a.tx-link { background: none !important; padding: 0 !important; }
            #journal-tree li.tx-li > i.jstree-icon { display: none; }
            #journal-tree .jstree-children { display: flex; flex-direction: column; gap: 8px; padding: 8px 0; }
        "#)) }

        div id="journal-tree" {}

        script src="https://code.jquery.com/jquery-3.7.1.min.js" {}
        script src="https://cdnjs.cloudflare.com/ajax/libs/jstree/3.3.16/jstree.min.js" {}

        script {
            (maud::PreEscaped(format!(r#"
                $(function() {{
                    $('#journal-tree').jstree({{
                        core: {{
                            data: {tree_json},
                            themes: {{ icons: true, dots: true }},
                        }},
                        plugins: []
                    }})
                    .on('select_node.jstree', function(e, data) {{
                        const href = data.node.a_attr?.href;
                        if (href) window.location.href = href;
                    }});
                }});
            "#)))
        }
    }
}

pub async fn transaction_list_page(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    let user = user::get_user(session)?;
    let authority = Authority::Direct(Actor::User(user.id));

    let journal_id_res = JournalId::from_str(&id);

    let content = html! {
        @if let Ok(journal_id) = journal_id_res && let Ok(Some(j_name)) = state.journal_service.get_name(journal_id).await {
            (render_transactions(journal_id, &j_name.to_string(), &authority, &state).await)
        } @else {
            p {
                "invalid journal"
            }
        }

        hr class="mt-8 mb-6 border-gray-300 dark:border-gray-600";

        div class="mt-10" {
            div class="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl p-6" {
                h3 class="text-lg font-semibold text-gray-900 dark:text-white mb-6" {
                    "Create New Transaction"
                }
            }

            @if let Ok(journal_id) = journal_id_res && let Ok(accounts) = state.account_service.get_all_accounts_in_journal(journal_id, &authority).await {
                @let journal_name = state.journal_service.get_name_from_res(journal_id_res.clone()).await.or_unknown();
                @let subjournals = state.journal_service.get_subjournals(journal_id, &authority).await.unwrap_or_default();
                @let has_subjournals = !subjournals.is_empty();
                form method="post" action=(format!("/journal/{}/transaction", id)) class="space-y-6" {
                    @for i in 0..4 {
                        div class="p-4 bg-gray-50 dark:bg-gray-700 rounded-lg" {
                            @if has_subjournals {
                                div class="grid grid-cols-4 gap-3 md:grid-cols-12" {
                                    div class="col-span-4 md:col-span-3" {
                                        label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                            "Journal"
                                        }
                                        select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400"
                                        name="entry_journal" {
                                            (render_journal_options(&subjournals, journal_id, &journal_name, 0))
                                        }
                                    }
                                    div class="col-span-4 md:col-span-5" {
                                        label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                            (if i < 2 {"Account"} else {"Account (Optional)"})
                                        }
                                        select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400"
                                        name="account" {
                                            option value="" { "Select account..." }
                                            (render_account_options(&accounts, None, 0))
                                        }
                                    }
                                    div class="col-span-3 md:col-span-3" {
                                        label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                            "Amount"
                                        }
                                        input class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400 text-right [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none [-moz-appearance:textfield]"
                                        type="number"
                                        step="0.01" min="0"
                                        placeholder="0.00"
                                        required[i < 2]
                                        name="amount";
                                    }
                                    div class="col-span-1 md:col-span-1" {
                                        label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                            "Type"
                                        }
                                        select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400"
                                        name="entry_type" {
                                            option value=(EntryType::Debit) { "Dr" }
                                            option value=(EntryType::Credit) { "Cr" }
                                        }
                                    }
                                }
                            } @else {
                                // No subjournals — journal is implicit, omit the column
                                input type="hidden" name="entry_journal" value=(id);
                                div class="grid grid-cols-4 gap-3 md:grid-cols-12" {
                                    div class="col-span-4 md:col-span-6" {
                                        label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                            (if i < 2 {"Account"} else {"Account (Optional)"})
                                        }
                                        select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400"
                                        name="account" {
                                            option value="" { "Select account..." }
                                            (render_account_options(&accounts, None, 0))
                                        }
                                    }
                                    div class="col-span-3 md:col-span-5" {
                                        label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                            "Amount"
                                        }
                                        input class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white placeholder:text-gray-400 dark:placeholder:text-gray-500 focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400 text-right [&::-webkit-outer-spin-button]:appearance-none [&::-webkit-inner-spin-button]:appearance-none [-moz-appearance:textfield]"
                                        type="number"
                                        step="0.01" min="0"
                                        placeholder="0.00"
                                        required[i < 2]
                                        name="amount";
                                    }
                                    div class="col-span-1 md:col-span-1" {
                                        label class="block text-sm font-medium text-gray-700 dark:text-gray-300 mb-2" {
                                            "Type"
                                        }
                                        select class="w-full rounded-md border border-gray-300 dark:border-gray-600 bg-white dark:bg-gray-800 px-3 py-2 text-gray-900 dark:text-white focus:border-indigo-500 focus:ring-indigo-500 dark:focus:border-indigo-400"
                                        name="entry_type" {
                                            option value=(EntryType::Debit) { "Dr" }
                                            option value=(EntryType::Credit) { "Cr" }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    div class="flex justify-between items-center pt-4 border-t border-gray-200 dark:border-gray-600" {
                        div class="text-sm text-gray-500 dark:text-gray-400" {
                            "Debits must equal credits within each journal"
                        }
                        button class="px-6 py-2 bg-indigo-600 text-white font-medium rounded-md hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:ring-offset-2 dark:bg-indigo-500 dark:hover:bg-indigo-400 dark:focus:ring-indigo-400 dark:ring-offset-gray-800" type="submit" {
                            "Create Transaction"
                        }
                    }
                }
            }

            @if let Some(e) = err.err {
                p {
                    (format!("An error occurred: {:?}", MonkestoError::decode(&e)))
                }
            }
        }
    };

    let wrapped_content = html! {
        div class="flex flex-col gap-6 mx-auto w-full max-w-4xl" {
            (content)
        }
    };

    Ok(layout::layout(
        Some(
            &state
                .journal_service
                .get_name_from_res(journal_id_res)
                .await
                .or_unknown(),
        ),
        true,
        Some(&id),
        wrapped_content,
    ))
}
