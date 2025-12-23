use super::account::AccountList;
use super::nav::TopBar;
use crate::api::main_api;
use crate::api::main_api::CreateJournal;
use leptos::prelude::*;

#[component]
pub fn HomePage() -> impl IntoView {
    let user_id_resource = Resource::new(
        move || (),
        |_| async move { main_api::get_user_id_from_session().await },
    );

    let journals_resource = Resource::new(
        move || (),
        move |_| async move { main_api::get_associated_journals().await },
    );

    let accounts_resource = Resource::new(
        move || (),
        move |_| async move { main_api::get_accounts().await },
    );

    let create_journal = ServerAction::<CreateJournal>::new();

    view! {
        <Suspense>
            {move || Suspend::new(async move {
                let user_id = match user_id_resource.await {
                    Ok(s) => s,
                    Err(_) => {
                        return (view! { <meta http-equiv="refresh" content="0; url=/login" /> },)
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
                        <TopBar journals=journals.clone() />
                        <h1>"Please select a journal to continue!"</h1>
                        <ActionForm action=create_journal>
                            <input type="hidden" name="user_id" value=user_id.to_string() />
                            <input
                                class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                name="journal_name"
                                type="text"
                                required
                                placeholder="journal name"
                            />
                            <button type="submit">"Create Journal!"</button>
                        </ActionForm>
                    }
                        .into_any();
                }
                let accounts = match accounts_resource.await {
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

                    <AccountList accounts=accounts journals=journals user_id=user_id />
                }
                    .into_any()
            })}
        </Suspense>
    }
}
