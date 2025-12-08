use crate::main_api::web_api;
use crate::main_api::web_api::CreateJournal;
use leptos::prelude::*;
use crate::nav::TopBar;
use crate::account::AccountList;

#[component]
pub fn HomePage() -> impl IntoView {
    use leptos::either::EitherOf5;

    let user_id_resource = Resource::new(
        move || (),
        |_| async move { web_api::get_user_id_from_session().await },
    );

    let journals_resource = Resource::new(
        move || (),
        move |_| async move { web_api::get_associated_journals(user_id_resource.await).await },
    );

    let accounts_resource = Resource::new(
        move || (),
        move |_| async move { web_api::get_accounts(user_id_resource.await).await },
    );

    let create_journal = ServerAction::<CreateJournal>::new();

    view! {
        <Suspense>
            {move || Suspend::new(async move {
                let user_id = match user_id_resource.await {
                    Ok(s) => s,
                    Err(_) => {
                        return EitherOf5::A(
                            view! { <meta http-equiv="refresh" content="0; url=/login" /> },
                        );
                    }
                };
                let journals = match journals_resource.await {
                    Ok(s) => s,
                    Err(e) => {
                        return EitherOf5::B(

                            view! {
                                <p>"An error occured while fetching journals: "{e.to_string()}</p>
                            },
                        );
                    }
                };
                if journals.selected.is_none() {
                    return EitherOf5::C(

                        view! {
                            <TopBar user_id=user_id journals=journals />
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
                        },
                    );
                }
                let accounts = match accounts_resource.await {
                    Ok(s) => s,
                    Err(e) => {
                        return EitherOf5::D(

                            view! {
                                <p>"An error occured while fetching accounts: "{e.to_string()}</p>
                            },
                        );
                    }
                };
                EitherOf5::E(
                    view! {
                        <TopBar journals=journals.clone() user_id=user_id />

                        <AccountList accounts=accounts journals=journals user_id=user_id />
                    },
                )
            })}
        </Suspense>
    }
}
