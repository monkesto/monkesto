use crate::main_api::return_types::*;
use crate::main_api::web_api;
use leptos::prelude::*;
use uuid::Uuid;

#[component]
pub fn TopBar(journals: Journals, user_id: Uuid) -> impl IntoView {
    use leptos::either::Either;
    use web_api::{LogOut, SelectJournal};
    let log_out_action = ServerAction::<LogOut>::new();
    let select_journal = ServerAction::<SelectJournal>::new();
    let username_resource = Resource::new(
        || (),
        move |_| async move { web_api::get_username_from_id(user_id).await },
    );

    view! {
        <div class="flex max-w-7xl mx-auto items-center flex-col px-4 py-4 gey-8">
            <Suspense>
                {move || Suspend::new(async move {
                    match username_resource.await {
                        Err(e) => {
                            Either::Left(
                                view! {
                                    <p>
                                        "An error occured while fetching username: "{e.to_string()}
                                    </p>
                                },
                            )
                        }
                        Ok(s) => {
                            Either::Right(view! { <h1 class="text-4xl">"Welcome, "{s}"!"</h1> })
                        }
                    }
                })}
            </Suspense>
            <a
                href="/transact"
                class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400 mx-3"
            >
                "Make a Transaction"
            </a>
            <a
                href="/"
                class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400 mx-3"
            >
                "Homepage"
            </a>
            <a
                href="/journal"
                class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400 mx-3"
            >
                "Transaction journal"
            </a>
            <a
                href="/invites"
                class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400 mx-3"
            >
                "Journal Invites"
            </a>

            <ActionForm action=log_out_action>
                <input type="hidden" name="user_id" value=user_id.to_string() />
                <button
                    class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400"
                    type="submit"
                >
                    "Log out"
                </button>

            </ActionForm>

            <br />

            <ActionForm action=select_journal>
                <input type="hidden" name="user_id" value=user_id.to_string() />
                <select name="journal_id">
                    <option value="">"-- Select a Journal --"</option>
                    {journals
                        .associated
                        .into_iter()
                        .map(|journal| {
                            view! {
                                <option
                                    value=journal.get_id().to_string()
                                    selected=journals
                                        .selected
                                        .clone()
                                        .is_some_and(|f| f.get_id() == journal.get_id())
                                >
                                    {journal.get_name().to_string()}
                                </option>
                            }
                        })
                        .collect_view()}
                </select>
                <button
                    class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400"
                    type="submit"
                >
                    "Select"
                </button>
            </ActionForm>
            {match select_journal.value().get() {
                Some(Err(e)) => {
                    Either::Left(
                        view! {
                            <p>"An error occured while switching journals: "{e.to_string()}</p>
                        },
                    )
                }
                _ => Either::Right(view! { "" }),
            }}
        </div>
    }
}
