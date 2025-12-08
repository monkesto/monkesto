use crate::event_sourcing::journal::Permissions;
use crate::main_api::return_types::*;
use crate::main_api::web_api;
use leptos::prelude::*;
use uuid::Uuid;

#[component]
fn AddAccount(user_id: Uuid, journal_id: Uuid) -> impl IntoView {
    use leptos::either::Either;
    let add_account = ServerAction::<web_api::AddAccount>::new();
    view! {
        <div class="flex flex-col items-center text-center px-10 py-10">
            <h1>"Create a new account"</h1>
            <ActionForm action=add_account>
                <input
                    class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                    type="text"
                    name="account_name"
                    required
                />

                <input type="hidden" name="user_id" value=user_id.to_string() />
                <input type="hidden" name="journal_id" value=journal_id.to_string() />

                <button
                    class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400"
                    type="submit"
                >
                    "Add"
                </button>
            </ActionForm>

            {move || {
                match add_account.value().get() {
                    Some(Err(e)) => {
                        Either::Left(
                            view! {
                                <p>
                                    "An error occured while creating the account: " {e.to_string()}
                                </p>
                            },
                        )
                    }
                    _ => Either::Right(view! { "" }),
                }
            }}

        </div>
    }
}

#[component]
pub fn AccountList(mut accounts: Vec<Account>, journals: Journals, user_id: Uuid) -> impl IntoView {
    use leptos::either::{Either, EitherOf4};
    let create_journal = ServerAction::<web_api::CreateJournal>::new();

    view! {
        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
            <ActionForm action=create_journal>
                <input type="hidden" name="user_id" value=user_id.to_string() />
                <input
                    class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                    name="journal_name"
                    type="text"
                    placeholder="journal name"
                    required
                />
                <button
                    type="submit"
                    class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400"
                >
                    "Create Journal!"
                </button>
            </ActionForm>

            {move || {
                match create_journal.value().get() {
                    Some(Err(e)) => {
                        Either::Left(
                            view! {
                                <p>
                                    "An error occured while creating the journal: " {e.to_string()}
                                </p>
                            },
                        )
                    }
                    _ => Either::Right(view! { "" }),
                }
            }}

            <h1 class="font-bold text-4xl">"Accounts"</h1>
        </div>
        <div class="mx-auto flex min-w-full flex-col items-center">
            <ul>
                {
                    accounts.sort_unstable_by_key(|account| account.name.clone());
                    accounts
                        .into_iter()
                        .map(|account| {
                            view! {
                                <li class="px-1 py-1 font-bold text-2xl">
                                    {account.name}"    "
                                    {format!(
                                        "${}.{:02} {}",
                                        account.balance.abs() / 100,
                                        account.balance.abs() % 100,
                                        if account.balance < 0 { "Dr" } else { "Cr" },
                                    )}
                                </li>
                            }
                        })
                        .collect_view()
                }
            </ul>
        </div>

        {move || {
            if let Some(selected) = journals.selected.clone() {
                match selected {
                    AssociatedJournal::Owned { id, .. } => {
                        EitherOf4::A(view! { <AddAccount user_id=user_id journal_id=id /> })
                    }
                    AssociatedJournal::Shared { id, tenant_info, .. } => {
                        if tenant_info.tenant_permissions.contains(Permissions::ADDACCOUNT) {
                            EitherOf4::B(view! { <AddAccount user_id=user_id journal_id=id /> })
                        } else {
                            EitherOf4::C(view! { "" })
                        }
                    }
                }
            } else {
                EitherOf4::D(view! { "" })
            }
        }}
    }
}