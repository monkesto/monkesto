use super::super::nav::TopBar;
use crate::api::main_api;
use crate::api::main_api::get_accounts;
use crate::api::return_types::*;
use leptos::prelude::*;

#[component]
pub fn Transact() -> impl IntoView {
    use main_api::{Transact, get_associated_journals, get_user_id_from_session};
    let user_id_resource =
        Resource::new(|| (), |_| async move { get_user_id_from_session().await });

    let journals_resource = Resource::new(
        || (),
        move |_| async move { get_associated_journals().await },
    );

    let accounts_resource = Resource::new(|| (), move |_| async move { get_accounts().await });

    let update_action = ServerAction::<Transact>::new();

    view! {
        <Suspense>
            {move || Suspend::new(async move {
                let user_id = match user_id_resource.await {
                    Ok(s) => s,
                    Err(_) => {
                        return view! { <meta http-equiv="refresh" content="0; url=/login" /> }
                            .into_any();
                    }
                };
                let journals = match journals_resource.await {
                    Ok(s) => s,
                    Err(e) => {
                        return view! {
                            <p>"An error occurred while fetching journals: "{e.to_string()}</p>
                        }
                            .into_any();
                    }
                };
                if journals.selected.is_none() {
                    return view! {
                        <TopBar journals=journals />
                        <h1 class="font-bold text-4xl">"please select a journal"</h1>
                    }
                        .into_any();
                }
                let mut accounts = match accounts_resource.await {
                    Ok(s) => s,
                    Err(e) => {
                        return view! {
                            <p>"An error occurred while fetching accounts: "{e.to_string()}</p>
                        }
                            .into_any();
                    }
                };
                view! {
                    <TopBar journals=journals.clone() />
                    <h1 class="font-bold text-4xl">"Transact"</h1>
                    <h2 class="font-bold text-2xl">"Credits/Debits"</h2>
                    <ActionForm action=update_action>
                        <input name="user_id" type="hidden" value=user_id.to_string() />

                        <input
                            name="journal_id"
                            type="hidden"
                            value=journals
                                .selected
                                .expect("the accounts resource should fail if this value is none")
                                .get_id()
                                .to_string()
                        />

                        {
                            let last_transaction = match update_action.value().get() {
                                Some(Err(e)) => {
                                    match KnownErrors::parse_error(e) {
                                        Some(
                                            KnownErrors::BalanceMismatch { attempted_transaction },
                                        ) => {
                                            Some(
                                                attempted_transaction
                                                    .into_iter()
                                                    .map(|update| (update.account_name, update.changed_by))
                                                    .collect::<std::collections::HashMap<String, i64>>(),
                                            )
                                        }
                                        _ => None,
                                    }
                                }
                                _ => None,
                            };
                            accounts.sort_unstable_by_key(|account| account.name.clone());
                            accounts
                                .into_iter()
                                .map(|account| {

                                    view! {
                                        <div class="flex items-center text-center px-10 py-10">
                                            <label class="block mb-2 font-medium">
                                                {account.name.to_string()}
                                            </label>
                                            <br />

                                            <div class="flex gap-4">
                                                <input
                                                    name="account_names[]"
                                                    type="hidden"
                                                    value=account.name.to_string()
                                                />

                                                <input
                                                    class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                                    name="balance_add_cents[]"
                                                    type="number"
                                                    inputmode="decimal"
                                                    step="0.01"
                                                    value=match &last_transaction {
                                                        Some(s) => {
                                                            if let Some(t) = s.get(&account.name) && t > &0 {
                                                                (t.abs() as f64 / 100.0).to_string()
                                                            } else {
                                                                "".to_string()
                                                            }
                                                        }
                                                        None => "".to_string(),
                                                    }
                                                    placeholder="0.00"
                                                />

                                                <input
                                                    class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                                    name="balance_remove_cents[]"
                                                    type="number"
                                                    inputmode="decimal"
                                                    step="0.01"
                                                    value=match &last_transaction {
                                                        Some(s) => {
                                                            if let Some(t) = s.get(&account.name) && t < &0 {
                                                                (t.abs() as f64 / 100.0).to_string()
                                                            } else {
                                                                "".to_string()
                                                            }
                                                        }
                                                        None => "".to_string(),
                                                    }
                                                    placeholder="0.00"
                                                />

                                            </div>

                                        </div>
                                    }
                                })
                                .collect_view()
                        }
                        <button
                            class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400"
                            type="submit"
                        >
                            "Submit"
                        </button>
                        <br />

                        {if let Some(Err(e)) = update_action.value().get() {

                            view! {
                                {if let Some(KnownErrors::BalanceMismatch { .. }) = KnownErrors::parse_error(
                                    e,
                                ) {

                                    view! {
                                        <p>"Please confirm that your credits match your debits"</p>
                                    }
                                        .into_any()
                                } else {

                                    view! {
                                        <p>
                                            "Please ensure that you filled out at least two fields."
                                        </p>
                                    }
                                        .into_any()
                                }}
                            }
                                .into_any()
                        } else {
                            view! { "" }.into_any()
                        }}

                    </ActionForm>
                }
                    .into_any()
            })}
        </Suspense>
    }
}
