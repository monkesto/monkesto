use crate::cuid::Cuid;
use crate::journal::commands::CreateJournal;
use crate::journal::handle_error::handle_error;
use crate::journal::layout::Layout;
use crate::journal::queries::{get_associated_journals, get_journal_owner};
use crate::unwrap_or_handle_error;
use leptos::prelude::*;

#[allow(dead_code)]
pub struct Journal {
    pub id: Cuid,
    pub name: String,
    pub creator_username: String,
    pub created_at: String,
}

#[component]
pub fn JournalList() -> impl IntoView {
    let journals_resource = Resource::new(
        move || (),
        |_| async move { get_associated_journals().await },
    );

    let create_journal = ServerAction::<CreateJournal>::new();

    view! {
        <Layout>
            <Suspense>
                {move || Suspend::new(async move {
                    let journals: Vec<(Cuid, String)> = unwrap_or_handle_error!(
                        journals_resource.await, "fetching journals"
                    )
                        .associated
                        .into_iter()
                        .map(|journal| (journal.get_id(), journal.get_name()))
                        .collect();
                    journals
                        .into_iter()
                        .map(|(journal_id, journal_name)| {

                            view! {
                                <a
                                    href=format!("/journal/{}", journal_id)
                                    class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                                >
                                    <h3 class="text-lg font-semibold text-gray-900 dark:text-white">
                                        {journal_name}
                                    </h3>
                                </a>
                            }
                        })
                        .collect_view()
                        .into_any()
                })} <hr class="mt-8 mb-6 border-gray-300 dark:border-gray-600" />
            </Suspense>

            <div class="mt-10">
                <ActionForm action=create_journal attr:class="space-y-6">
                    <div>
                        <label
                            for="journal_name"
                            class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100"
                        >
                            "Create New Journal"
                        </label>
                        <div class="mt-2">
                            <input
                                id="journal_name"
                                type="text"
                                name="journal_name"
                                required
                                class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                            />
                        </div>
                    </div>
                    <div>
                        <button
                            type="submit"
                            class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500"
                        >
                            "Create Journal"
                        </button>
                    </div>
                </ActionForm>
            </div>
        </Layout>
    }
}

#[component]
pub fn JournalDetail() -> impl IntoView {
    use leptos_router::hooks::use_params_map;

    let params = use_params_map();

    let journals_resource = Resource::new(
        move || (),
        |_| async move { get_associated_journals().await },
    );

    view! {
        <Suspense>
            {move || Suspend::new(async move {
                let journal_id = move || params.get().get("id").unwrap_or_default().to_string();
                let journals = unwrap_or_handle_error!(
                    journals_resource.await, "fetching journals"
                );
                let Some(journal) = journals
                    .associated
                    .into_iter()
                    .find(|j| { j.get_id().to_string() == journal_id() }) else {
                    return view! { <p>"Unable to find journal"</p> }.into_any()
                };
                let journal_owner_resource = Resource::new(
                    move || (),
                    move |_| async move { get_journal_owner(journal_id()).await },
                );
                let journal_owner = journal_owner_resource.await;
                view! {
                    <Layout
                        page_title=journal.clone().get_name()
                        show_switch_link=true
                        journal_id=journal_id()
                    >

                        <a
                            href=format!("/journal/{}/transaction", journal_id())
                            class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                        >
                            <h3 class="text-lg font-semibold text-gray-900 dark:text-white">
                                "Transactions"
                            </h3>
                        </a>

                        <a
                            href=format!("/journal/{}/account", journal_id())
                            class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                        >
                            <h3 class="text-lg font-semibold text-gray-900 dark:text-white">
                                "Accounts"
                            </h3>
                        </a>

                        <a
                            href=format!("/journal{}/person", journal_id())
                            class="block p-4 bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-xl hover:bg-gray-50 dark:hover:bg-gray-700 transition-colors"
                        >
                            <h3 class="text-lg font-semibold text-gray-900 dark:text-white">
                                "People"
                            </h3>
                        </a>

                        <div class="mt-6 p-4 bg-gray-50 dark:bg-gray-800 rounded-lg">
                            <div class="space-y-2">
                                <div class="text-sm text-gray-600 dark:text-gray-400">
                                    "Created by "
                                    {match journal_owner {
                                        Err(e) => format!("error: {}", e),
                                        Ok(None) => "unknown user".to_string(),
                                        Ok(Some(s)) => s,
                                    }} " on "
                                    {journal
                                        .get_created_at()
                                        .with_timezone(&chrono_tz::America::Chicago)
                                        .format("%Y-%m-%d %H:%M:%S %Z")
                                        .to_string()}
                                </div>
                            </div>
                        </div>

                    </Layout>
                }
                    .into_any()
            })}
        </Suspense>
    }
}
