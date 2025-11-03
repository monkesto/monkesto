use leptos::prelude::*;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
};

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
                        <Route path=StaticSegment("/transact") view=Transact/>
                        <Route path=StaticSegment("/journal") view=GeneralJournal/>
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
    use crate::api::CreateAccount;
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
                _ => Either::Right( view! {} )
            }
    }
    </div>
        }
}

#[component]
fn ClientLogin() -> impl IntoView {
    use crate::api::{is_logged_in, Login};
    use leptos::either::{Either, EitherOf3};
    let login = ServerAction::<Login>::new();
    let logged_in = Resource::new(|| (), |_| async { is_logged_in().await });

    view! {
            <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
            <Suspense>
                {move || match logged_in.get() {
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
                    None => EitherOf3::C (view! {})
                }
            }
            </Suspense>
            {move || match login.value().get() {
                Some(Err(e)) => Either::Left( view! {<div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>{e.to_string()}</p></div>}),
                _ => Either::Right(view! {}),
            }
        }
    </div>
        }
}

#[component]
fn TopBar() -> impl IntoView {
    use crate::api::LogOut;
    let log_out_action = ServerAction::<LogOut>::new();
    view! {
                    <div class="flex max-w-7xl mx-auto items-center flex-col">
                        <div class="flex items-center justify-between px-4 py-4">
                            <div class="flex gap-8">
                                <a href="/transact" class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400 mx-3">
                                    "Make a Transaction"
                                </a>
                                <a href="/" class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400 mx-3">
                                    "Home"
                                </a>
                                <a href="/journal" class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400 mx-3">
                                    "Transaction journal"
                                </a>
                                <br/>
                                <ActionForm action=log_out_action>
                                    <button class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400" type="submit">"Log out"</button>
                                </ActionForm>
                            </div>
                        </div>
                    </div>
    }
}

#[component]
fn HomePage() -> impl IntoView {
    view! {
        <head>
        <TopBar/>
        <AccountList/>
        </head>
    }
}

#[component]
fn AccountList() -> impl IntoView {
    use crate::api::{get_accounts, is_logged_in, ShareAccount};
    use leptos::either::{Either, EitherOf3};

    let accounts = Resource::new(move || (), |_| async move { get_accounts().await });
    let logged_in = Resource::new(move || (), |_| async move { is_logged_in().await });
    let share_account = ServerAction::<ShareAccount>::new();

    view! {
        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
            <h1 class="font-bold text-4xl">"Accounts"</h1>
        </div>
        <Suspense>
        {move || {
            let login_state = logged_in.get();
            let accounts_state = accounts.get();
            let share_state = share_account.value().get();

            match login_state {
                None => EitherOf3::A ( view!{
                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"checking if you are logged in"</p></div>
                }),
                Some(Err(_)) => EitherOf3::B( view! {
                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><meta http-equiv="refresh" content="0; url=/login"/></div>
                }),
                Some(Ok(_)) => EitherOf3::C( view! {})};

            match accounts_state {
                None => EitherOf3::A( view! { <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"Loading accounts..."</p></div> }),
                Some(Err(e)) => EitherOf3::B(
                    view! { <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"Error loading accounts: " {e.to_string()}</p></div>}),
                Some(Ok(s)) => EitherOf3::C(
                    if s.is_empty() {
                        Either::Left( view! {
                            <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"No accounts yet. Add one below!"</p>
                            <AddAccount/>
                            </div>

                        })
                        } else {
                            Either::Right(
                            view! {
                                <div class="mx-auto flex min-w-full flex-col items-center">
                                    <ul>
                                        {s.into_iter()
                                            .map(|n| view! {
                                                <div style=move || if n.2 {
                                                    "color: red;"
                                                } else {
                                                    ""
                                                }>
                                                <ActionForm action=share_account>
                                                <li class = "px-1 py-1 font-bold text-2xl">
                                                    {n.0}"     "{format!("{}${}.{:02}", if n.1 < 0 {"-"} else {""}, (n.1.abs() / 100), ((n.1).abs() % 100))}
                                                            <input
                                                            type = "hidden"
                                                            name = "account_id"
                                                            value = 0
                                                            // TODO: fix the value

                                                            />
                                                            <input
                                                                class = "shadow appearance-none border rounded text-gray-700 leading-tight focus:outline-none focus:shadow-outline max-w-xs"
                                                                type="text"
                                                                name="username"
                                                                placeholder = "username"
                                                                required
                                                            />
                                                            <button class="mt-3 rounded bg-purple-900 font-bold text-white hover:bg-blue-400" type="submit">"Share"</button>
                                                </li></ActionForm></div>})
                                            .collect_view()}
                                    </ul>
                                    <AddAccount/>
                                    {match share_state{
                                        Some(Err(e)) => Either::Left( view!{<div class="mx-auto flex min-w-full flex-col items-center"><p>{e.to_string()}</p></div>}),
                                        _ => Either::Right( view!{} )
                                    }}
                                </div>
                            })
                        }
                    )
            }
        }}
        </Suspense>
    }
}

#[component]
fn AddAccount() -> impl IntoView {
    let add_account = ServerAction::<crate::api::AddAccount>::new();
    view! {
            <div class="flex flex-col items-center text-center px-10 py-10">
                <h1>"Create a new account"</h1>
                    <ActionForm action=add_account>
                        <input
                            class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                            type="text"
                            name="title"
                            required
                        />
                        <button class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400" type="submit">"Add"</button>
                    </ActionForm>
            </div>
    }
}

#[component]
fn Transact() -> impl IntoView {
    use crate::api::{get_accounts, is_logged_in, Transact};
    use leptos::either::{Either, EitherOf3};
    let items_resource = Resource::new(|| (), |_| async { get_accounts().await });
    let logged_in_resource = Resource::new(|| (), |_| async { is_logged_in().await });
    let update_action = ServerAction::<Transact>::new();

    view! {
        <TopBar/>
        <Suspense fallback=|| view! { <p>"Loading..."</p> }>
            {move || {
                let login_state = logged_in_resource.get();
                let _items_state = items_resource.get();

                match login_state {
                    None => EitherOf3::A( view! {
                        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"checking if you are logged in"</p></div>
                    }),
                    Some(Err(_)) => EitherOf3::B( view! {
                        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><meta http-equiv="refresh" content="0; url=/login"/></div>
                    }),
                    Some(Ok(_)) => EitherOf3::C( view! {}),
                }
            }}
            {move || match logged_in_resource.get() {
                Some(Err(_)) => EitherOf3::A(
                    view! {
                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><meta http-equiv="refresh" content="0; url=/login"/></div>
                }),

                None =>  EitherOf3::B( view! {
                <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"loading accounts..."</p></div>}),

                Some(Ok(_)) => EitherOf3::C(
                    {items_resource.get().map(|result| {
                        match result {
                            Ok(items) => {
                                if items.len() < 2 {
                                   return EitherOf3::A(view! {
                                        <div class="flex flex-col items-center text-center px-10 py-10"><p>"You must have two accounts in order to transact!"</p></div>
                                    })
                                }
                                EitherOf3::B( view! {
                                    <div class="flex flex-col items-center text-center px-10 py-10">
                                    <h1 class="font-bold text-4xl">"Make a transaction"</h1>
                                    <p>"Please enter your values in cents"</p>
                                    <br/>
                                    <h2 class = "font-bold text-3xl">"Credit/Debit"</h2>
                                    <ActionForm action=update_action>

                                            {items.into_iter().map(|fields| view!{
                                                <div class="flex items-center text-center px-10 py-10">
                                                <label class="block mb-2 font-medium">{fields.0.to_string()}</label>

                                                <br/>

                                                <div class="flex gap-4">
                                                <input
                                                name = "acc_ids[]"
                                                type = "hidden"
                                                value = fields.1
                                                />

                                                <input
                                                class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                                name = "balance_add_cents[]"
                                                type="number"
                                                placeholder = "0"/>

                                                <input
                                                class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                                name = "balance_remove_cents[]"
                                                type="number"
                                                placeholder = "0"/>
                                                </div>
                                            </div>

                                            }).collect_view()}

                                       <button class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400" type="submit">"Submit"</button>

                                       <br/>

                                   {move || match update_action.value().get() {

                                           Some(Err(e)) => Either::Left(
                                               view! {
                                                   <p>{e.to_string()}</p>
                                               }),

                                           _ => Either::Right( view! {}),

                                       }
                                   }

                                    </ActionForm>

                                    </div>
                                })

                            }
                           Err(e) => EitherOf3::C( view! {<div class="flex flex-col items-center text-center px-10 py-10"><p>"Error: "{e.to_string()}</p></div>})
                        }
                    })}
                )
            }
        }
        </Suspense>
    }
}

#[component]
fn GeneralJournal() -> impl IntoView {
    use crate::api::{is_logged_in, package_transactions};
    use chrono::TimeZone;
    use leptos::either::EitherOf3;

    let transactions_resource = Resource::new(|| (), |_| async { package_transactions().await });
    let logged_in_resource = Resource::new(|| (), |_| async { is_logged_in().await });

    view! {

            <TopBar/>
            <Suspense fallback=|| view! { <p>"Loading transaction history..."</p> }>

            {move || {
                let login_state = logged_in_resource.get();

                match login_state {
                    None => EitherOf3::A( view! {
                        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"checking if you are logged in"</p></div>
                    }),
                    Some(Err(_)) => EitherOf3::B( view! {
                        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><meta http-equiv="refresh" content="0; url=/login"/></div>
                    }),
                    Some(Ok(_)) => EitherOf3::C( view! {} )

            }};

            {move || transactions_resource.get().map(|transactions|{
                   match transactions {
                       Ok(transactions) => {
                       if transactions.is_empty() {
                           EitherOf3::A(
                           view! {
                               <div class="flex flex-col items-center text-center px-10 py-10"><p>"no transactions yet"</p></div>
                           })
                       } else {
                       EitherOf3::B( view! {
                           <div class="flex flex-col items-center text-center px-10 py-10">
                                <h1 class="font-bold text-4xl">"Transactions"</h1>
                                <ul>
                                    {transactions.iter().map(|packaged_transaction| {
                                         //using utc because we can't get users' timezones without JS
                                         view! {
                                        <div class="flex flex-col items-center text-center px-10 py-10">
                                            <li>
                                                <h2 font-bold text-xl>
                                                {chrono::Utc.timestamp_opt(packaged_transaction.0.1, 0).unwrap().to_string()}":"
                                                </h2>
                                                <ul>
                                                    {packaged_transaction.1.iter().map(|partial_transaction| {
                                                        view! {
                                                        <li>{"dummy_account".to_string()} " : $" {partial_transaction.1.abs()/100}"."{partial_transaction.1.abs()%100} " " {if partial_transaction.1 < 0 {"Dr".to_string()} else {"Cr".to_string()}} </li>
                                                        // TODO: USE ACTUAL ACCOUNT NAME
                                                        }
                                                    }).collect_view()}
                                                </ul>
                                            </li>
                                        </div>
                                        }
                                    }).collect_view()}
                                </ul>
                           </div>
                       })}},
                       Err(e) => EitherOf3::C( view! {
                           <div class="flex flex-col items-center text-center px-10 py-10"><p>{e.to_string()}</p></div>
                       }),
                   }
                })}}

            </Suspense>
    }
}
