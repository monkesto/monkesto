//use std::time::{Duration, UNIX_EPOCH};

use crate::event_sourcing::journal::Permissions;
use crate::main_api::return_types::*;
use crate::main_api::web_api;
use crate::main_api::web_api::CreateJournal;
//use chrono::DateTime;
use leptos::prelude::*;
use leptos_meta::{MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::{
    StaticSegment,
    components::{Route, Router, Routes},
};
use uuid::Uuid;

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body>
                <App/>
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/prototype.css"/>

        // sets the document title
        <Title text="Double-book accounting"/>

        // content for this welcome page

        <Router>
            <main>
                <head>
                    <Routes fallback=|| "Page not found.".into_view()>
                        <Route path=StaticSegment("") view=HomePage/>
                        //<Route path=StaticSegment("/transact") view=Transact/>
                        //<Route path=StaticSegment("/journal") view=GeneralJournal/>
                        <Route path=StaticSegment("/login") view=ClientLogin/>
                        <Route path=StaticSegment("/signup") view=ClientSignUp/>
                    </Routes>
                </head>
            </main>
        </Router>
    }
}

#[component]
fn ClientSignUp() -> impl IntoView {
    use crate::main_api::web_api::CreateAccount;
    use leptos::either::Either;
    let signup = ServerAction::<CreateAccount>::new();

    view! {
            <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
                <Suspense>
                            <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">

                                <br/>

                                <ActionForm action=signup>

                                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">

                                <input
                                    class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                    type="text"
                                    name="username"
                                    placeholder="username"
                                    required
                                />
                                <br/>
                                <input
                                    class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                    type="password"
                                    name="password"
                                    placeholder="password"
                                    required
                                />
                                <br/>
                                <input
                                    class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                    type="password"
                                    name="confirm_password"
                                    placeholder="confirm password"
                                    required
                                />
                                <br/>
                                <button class="mt-3 rounded bg-purple-900 px-10 py-2 font-bold text-white hover:bg-blue-400" type="submit">"Sign up"</button>
                                </div>
                                </ActionForm>
                                <a href = "/login" class="mt-3 rounded bg-purple-900 px-10 py-2 font-bold text-white hover:bg-blue-400" type="submit">"Have an account? Sign in"</a>
                        </div>

            </Suspense>

            {move || match signup.value().get() {
                Some(Err(e)) => Either::Left( view! {<div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>{e.to_string()}</p></div>}),
                _ => Either::Right( view! {""} )
            }
    }
    </div>
        }
}

#[component]
fn ClientLogin() -> impl IntoView {
    use crate::main_api::web_api::{Login, get_user_id_from_session};
    use leptos::either::{Either, EitherOf3};

    let login = ServerAction::<Login>::new();
    let logged_in = Resource::new(|| (), |_| async { get_user_id_from_session().await }); // this throws an error if the database can't find an account associated with the session

    view! {
            <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
            <Suspense>
                {move || match logged_in.get() { // redirect to the homepage if the user's session id is already associated with an account
                    Some(Ok(_)) => EitherOf3::A( view! {
                        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><meta http-equiv="refresh" content="0; url=/"/></div>
                    }),
                    Some(Err(_)) => EitherOf3::B( view! {
                            <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">

                                <br/>

                                <ActionForm action=login>

                                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">

                                <input
                                    class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                    type="text"
                                    name="username"
                                    placeholder="username"
                                    required
                                />
                                <br/>
                                <input
                                    class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                    type="password"
                                    name="password"
                                    placeholder="password"
                                    required
                                />
                                <br/>
                                <button class="mt-3 rounded bg-purple-900 px-10 py-2 font-bold text-white hover:bg-blue-400" type="submit">"Login"</button>
                                </div>
                                </ActionForm>
                                <a href = "/signup" class="mt-3 rounded bg-purple-900 px-10 py-2 font-bold text-white hover:bg-blue-400" type="submit">"Don't have an account? Sign up"</a>
                        </div>
                    }),
                    None => EitherOf3::C (view! {""})
                }
            }
            </Suspense>
            {move || match login.value().get() {
                Some(Err(e)) => Either::Left( view! {<div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>{e.to_string()}</p></div>}),
                _ => Either::Right(view! {""}),
            }
        }
    </div>
        }
}

#[component]
fn TopBar(journals: Journals, user_id: Uuid) -> impl IntoView {
    use web_api::{LogOut, SelectJournal};
    let log_out_action = ServerAction::<LogOut>::new();
    let select_journal = ServerAction::<SelectJournal>::new();

    view! {
            <div class="flex max-w-7xl mx-auto items-center flex-col px-4 py-4 gey-8">
                        <a href="/transact" class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400 mx-3">
                            "Make a Transaction"
                        </a>
                        <a href="/" class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400 mx-3">
                            "Homepage"
                        </a>
                        <a href="/journal" class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400 mx-3">
                            "Transaction journal"
                        </a>
                        <a href="/invites" class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400 mx-3">
                            "Journal Invites"
                        </a>

                        <ActionForm action=log_out_action>
                            <input type="hidden" name="user_id" value=user_id.to_string()/>
                            <button class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400" type="submit">"Log out"</button>

                        </ActionForm>

                        <br/>

                        <ActionForm action=select_journal>
                        <input
                        type="hidden"
                        name="user_id"
                        value=user_id.to_string()
                        />
                        <select name="journal_id">
                            <option value="">"-- Select a Journal --"</option>
                            {
                                journals.associated.into_iter().map(|journal| view! {
                                    <option value=journal.get_id().to_string() selected=journals.selected.clone().is_some_and(|f| f.get_id()==journal.get_id())>{journal.get_name().to_string()}</option>
                                }).collect_view()
                            }
                        </select>
                        <button class = "mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400" type="submit">"Select"</button>
                        </ActionForm>
            </div>
    }
}

#[component]
fn AddAccount(user_id: Uuid, journal_id: Uuid) -> impl IntoView {
    use leptos::either::Either;
    let add_account = ServerAction::<web_api::AddAccount>::new();
    view! {
            <div class="flex flex-col items-center text-center px-10 py-10">
                <h1>"Create a new account"</h1>
                    <ActionForm action=add_account>
                        <input
                            class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                            type="text"
                            name="account_name"
                            required
                        />

                        <input
                            type = "hidden"
                            name = "user_id"
                            value = user_id.to_string()
                        />
                        <input
                            type = "hidden"
                            name = "journal_id"
                            value = journal_id.to_string()
                        />

                        <button class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400" type="submit">"Add"</button>
                    </ActionForm>

                    {
                        move || {
                            match add_account.value().get() {
                                Some(Err(e)) => Either::Left(view! {
                                    <p>"An error occured: " {e.to_string()}</p>
                                }),
                                _ => Either::Right(view! {""})
                            }
                        }
                    }

            </div>
    }
}

#[component]
fn AccountList(accounts: Vec<Account>, journals: Journals, user_id: Uuid) -> impl IntoView {
    use leptos::either::EitherOf4;

    view! {
        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
            <h1 class="font-bold text-4xl">"Accounts"</h1>
        </div>
            <div class="mx-auto flex min-w-full flex-col items-center">
                <ul>
                    {
                        accounts.into_iter().map(|account| view! {
                            <li class="px-1 py-1 font-bold text-2xl">
                                {account.name}"    " {format!("${}.{:02} {}", account.balance.abs()/100, account.balance.abs() % 100, if account.balance < 0 {"Dr"} else {"Cr"})}
                            </li>
                        }).collect_view()
                    }
                </ul>
            </div>


        {move || {
            if let Some(selected) = journals.selected.clone() {
                match selected {
                    AssociatedJournal::Owned { id, .. } => EitherOf4::A(view! {<AddAccount user_id=user_id journal_id=id/>}),
                    AssociatedJournal::Shared { id, tenant_info, .. } => {
                        if tenant_info.tenant_permissions.contains(Permissions::ADDACCOUNT) {
                            EitherOf4::B(view! { <AddAccount user_id=user_id journal_id=id/> })
                        } else {
                            EitherOf4::C(view! {""})
                        }
                    }
                }
            } else {
                EitherOf4::D( view! {""} )
            }
        }}
    }
}

#[component]
fn HomePage() -> impl IntoView {
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
            {move | |
                Suspend::new(async move {
                    let user_id = match user_id_resource.await {
                        Ok(s) => s,
                        Err(_) => return EitherOf5::A(view! {<meta http-equiv="refresh" content="0; url=/login"/>})
                    };

                    let journals = match journals_resource.await {
                        Ok(s) => s,
                        Err(e) => return EitherOf5::B(view! {<p>"An error occured while fetching journals: "{e.to_string()}</p>})
                    };

                    if journals.selected.is_none() {
                        return EitherOf5::C(view! {
                            <TopBar user_id=user_id journals=journals/>
                            <h1>"Please select a journal to continue!"</h1>
                            <ActionForm action=create_journal>
                                <input
                                    type="hidden"
                                    name="user_id"
                                    value=user_id.to_string()
                                    />
                                    <input
                                    name="journal_name"
                                    type="text"
                                    placeholder="journal name"
                                        />
                                    <button
                                    type="submit"
                                    >"Create Journal!"</button>
                            </ActionForm>
                        })
                    }

                    let accounts = match accounts_resource.await {
                        Ok(s) => s,
                        Err(e) => return EitherOf5::D(view! {<p>"An error occured while fetching accounts: "{e.to_string()}</p>})
                    };
                    EitherOf5::E(view! {
                        <TopBar journals=journals.clone() user_id=user_id/>
                        <AccountList accounts=accounts journals=journals user_id=user_id/>
                    })
                })
            }
        </Suspense>
    }
}

/*
#[component]
fn Transact() -> impl IntoView {
    use leptos::either::{Either, EitherOf8};
    use web_api::{Transact, get_associated_journals, get_user_id_from_session};
    let user_id_resource =
        Resource::new(|| (), |_| async move { get_user_id_from_session().await });

    let update_action = ServerAction::<Transact>::new();

    view! {
        <Suspense>
            {move || {
                match user_id_resource.get() {
                    None => EitherOf8::A(view! {<p>"Unable to fetch user id"</p>}),

                    Some(Err(_)) => EitherOf8::B(view! {<meta http-equiv="refresh" content="0; url=/login"/>}),

                    Some(Ok(user_id)) => {
                        let journals_resource = Resource::new(|| (), move |_| async move {get_associated_journals(user_id).await});

                        match journals_resource.get() {
                            None => EitherOf8::C(view! {<p>"Unable to fetch journals"</p>}),

                            Some(Err(e)) => EitherOf8::D(view! {<p>"An error occured while fetching journals: " {e.to_string()}</p>}),

                            Some(Ok(journals)) => {
                                let accounts_resource = Resource::new(|| (), move |_| async move {web_api::get_accounts(user_id).await});

                                match accounts_resource.get() {
                                    None => EitherOf8::E(view! {<p>"Unable to fetch accounts"</p>}),

                                    Some(Err(e)) => {if let Some(KnownErrors::InvalidJournal) = KnownErrors::parse_error(e.clone()) {
                                        EitherOf8::F(view! {<p>"Please select a journal!"</p>})} else {
                                            EitherOf8::G(view! {<p>"An error occured while fetching accounts: " {e.to_string()}</p>})
                                        }
                                    },

                                    Some(Ok(accounts)) => EitherOf8::H(view! {
                                        <TopBar journals=journals.clone() user_id=user_id/>
                                        <h1 class="font-bold text-4xl">"Transact"</h1>
                                        <ActionForm action=update_action>

                                        <input
                                            name="user_id"
                                            type="hidden"
                                            value=user_id.to_string()
                                            />

                                            <input
                                            name="journal_id"
                                            type="hidden"
                                            value=journals.selected.expect("fetching the accounts resource should fail if this value is none").get_id().to_string()
                                            />

                                            {
                                                accounts.into_iter().map(|account| view! {
                                                    <div class="flex items-center text-center px-10 py-10">
                                                        <label class="block mb-2 font-medium">{account.name.to_string()}</label>
                                                        <br/>
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
                                                            placeholder="0"
                                                            />

                                                            <input
                                                            class="shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                                            name="balance_remove_cents[]"
                                                            type="number"
                                                            placeholder="0"
                                                            />
                                                        </div>
                                                    </div>
                                                }).collect_view()
                                            }

                                        <button class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400" type="submit">"Submit"</button>
                                        <br/>

                                        {
                                            if let Some(Err(e)) = update_action.value().get() {
                                                Either::Left(view!{
                                                    <p>"An error occured while sumbitting your transaction: " {e.to_string()}</p>
                                                })
                                            } else {
                                                Either::Right(view! {""})
                                            }
                                        }

                                        </ActionForm>
                                    })

                                }
                            }
                        }
                    }
                }
            }}
        </Suspense>
    }
}

#[component]
fn GeneralJournal() -> impl IntoView {
    use crate::main_api::web_api::{
        get_associated_journals, get_transactions, get_user_id_from_session,
    };
    use leptos::either::EitherOf8;

    let user_id_resource =
        Resource::new(|| (), |_| async move { get_user_id_from_session().await });

    view! {

        <Suspense>
            {move || {
                match user_id_resource.get() {
                    None => EitherOf8::A(view! {<p>"Unable to fetch user id"</p>}),

                    Some(Err(_)) => EitherOf8::B(view! {<meta http-equiv="refresh" content="0; url=/login"/>}),

                    Some(Ok(user_id)) => {
                        let journals_resource = Resource::new(|| (), move |_| async move {get_associated_journals(user_id).await});
                        match journals_resource.get() {
                            None => EitherOf8::C(view! {<p>"Unable to fetch journals"</p>}),

                            Some(Err(e)) => EitherOf8::D(view! {<p>"An error occured while fetching journals: "{e.to_string()}</p>}),

                            Some(Ok(journals)) => {
                                if let Some(selected_journal) = &journals.selected {
                                    let selected = selected_journal.get_id();
                                    let transactions_resource = Resource::new(|| (), move |_| async move {get_transactions(user_id, selected).await});
                                    match transactions_resource.get() {
                                        None => EitherOf8::E(view! {<p>"Unable to fetch transactions"</p>}),

                                        Some(Err(e)) => EitherOf8::F(view! {<p>"An error occured while fetching transactions: "{e.to_string()}</p>}),

                                        Some(Ok(transactions)) => {
                                            EitherOf8::G(view! {
                                                <TopBar journals=journals.clone() user_id=user_id/>
                                                <div class="flex flex-col items-center text-center px-10 py-10">
                                                    <h1 class="font-bold text-4xl">"General Journal"</h1>
                                                    <ul>
                                                        {
                                                            transactions.into_iter().map(|transaction| view! {
                                                                <div class="flex flex-col items-center text-center px-10 py-10">
                                                                    <li>
                                                                        <h2 class="font-bold text-xl">{DateTime::<chrono::Utc>::from(UNIX_EPOCH.checked_add(Duration::from_secs(transaction.timestamp as u64)).unwrap_or(UNIX_EPOCH)).to_string()}":"</h2>
                                                                        <br/>
                                                                        <ul>
                                                                            {
                                                                                transaction.transaction.updates.into_iter().map(|update| view! {
                                                                                    <li>
                                                                                    {update.account_name} " : $" {update.changed_by.abs()/100} "." {update.changed_by.abs()%100} " " {if update.changed_by < 0 {"Dr".to_string()} else {"Cr".to_string()}}
                                                                                    </li>
                                                                                }).collect_view()
                                                                            }
                                                                        </ul>
                                                                    </li>
                                                                </div>
                                                            }).collect_view()
                                                        }
                                                    </ul>
                                                </div>
                                            })
                                        }
                                    }
                                } else {
                                    EitherOf8::H(view! {<p>"Please select a journal!"</p>})
                                }
                            }
                        }
                    }
                }
            }}
        </Suspense>
    }
}
*/
