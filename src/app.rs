#[cfg(feature = "ssr")]
use crate::api::{get_accounts, AddAccount};
use leptos::{html::P, prelude::*};

#[cfg(feature = "ssr")]
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
};

#[cfg(feature = "ssr")]
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

#[cfg(feature = "ssr")]
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

#[cfg(feature = "ssr")]
#[component]
fn ClientSignUp() -> impl IntoView {
    use crate::api::{is_logged_in, CreateAccount};
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
            None => return view! {<div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p></p></div>}.into_view(),
            Some(Ok(_)) => return view! {<div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p></p></div>}.into_view(),
            Some(Err(e)) => return view! {<div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>{e.to_string()}</p></div>}.into_view(),
        }
}
</div>
    }}


#[cfg(feature = "ssr")]
#[component]
fn ClientLogin() -> impl IntoView {
    use crate::api::{is_logged_in, Login};
    let login = ServerAction::<Login>::new();
    let logged_in = Resource::new(|| (), |_| async { is_logged_in().await });

    view! {
        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
        <Suspense>
            {move || match logged_in.get() {
                Some(Ok(_)) => {
                    return view! {
                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><meta http-equiv="refresh" content="0; url=/"/></div>
                }.into_view()},
                Some(Err(_)) => {
                    return view! {
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
                }.into_view()},
                None => return view! {
                <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"loading..."</p></div>}.into_view(),
                }
            }
        </Suspense>
        {move || match login.value().get() {
            None => return view! {<div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p></p></div>}.into_view(),
            Some(Ok(_)) => return view! {<div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p></p></div>}.into_view(),
            Some(Err(e)) => return view! {<div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>{e.to_string()}</p></div>}.into_view(),
        }
}
</div>
    }
}

#[cfg(feature = "ssr")]
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

/// Renders the home page of your application.
#[cfg(feature = "ssr")]
#[component]
fn HomePage() -> impl IntoView {
    view! {
        <head>
        <TopBar/>
        <AccountList/>
        </head>
    }
}

#[cfg(feature = "ssr")]
#[component]
fn AccountList() -> impl IntoView {
    use crate::api::is_logged_in;

    let accounts = Resource::new(move || (), |_| async move { get_accounts().await });
    let logged_in = Resource::new(move || (), |_| async move { is_logged_in().await });

    view! {
        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
            <h1 class="font-bold text-4xl">"Accounts"</h1>
        </div>
        <Suspense>
        {move || {
            let login_state = logged_in.get();
            let accounts_state = accounts.get();

            match login_state {
                None => view!{
                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"checking if you are logged in"</p></div>
                }.into_view(),
                Some(Err(_)) => return view! {
                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><meta http-equiv="refresh" content="0; url=/login"/></div>
                }.into_view(),
                Some(Ok(_)) =>
                    view! {
                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p></p></div>}.into_view(),
                };

            match accounts_state {
                None => view! { <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"Loading accounts..."</p></div> }.into_view(),
                Some(Err(e)) => {
                    view! {
                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"Error loading accounts: " {e.to_string()}</p></div>}.into_view()},
                Some(Ok(s)) => {
                    if s.is_empty() {
                        view! {
                            <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"No accounts yet. Add one below!"</p>
                            <AddAccount/>
                            </div>

                        }.into_view()
                        } else {
                            view! {
                                <div class="mx-auto flex min-w-full flex-col items-center">
                                    <ul>
                                        {s.into_iter()
                                            .map(|n| view! { <li class = "px-1 py-1 font-bold text-2xl">{n.title}"     "{format!("{}${}.{:02}", if n.balance_cents < 0 {"-"} else {""}, (n.balance_cents.abs() / 100), ((n.balance_cents).abs() % 100))}</li>})
                                            .collect_view()}
                                    </ul>
                                    <AddAccount/>
                                </div>

                            }.into_view()
                        }
                    }
            }
        }}
        </Suspense>
    }
}

#[cfg(feature = "ssr")]
#[component]
fn AddAccount() -> impl IntoView {
    let add_account = ServerAction::<AddAccount>::new();
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

#[cfg(feature = "ssr")]
#[component]
fn Transact() -> impl IntoView {
    use crate::{api::is_logged_in, api::Transact, types::TransactionResult};
    let items_resource = Resource::new(|| (), |_| async { get_accounts().await });
    let logged_in_resource = Resource::new(|| (), |_| async { is_logged_in().await });
    let update_action = ServerAction::<Transact>::new();

    view! {
        <TopBar/>
        <Suspense fallback=|| view! { <p>"Loading..."</p> }>
            {move || {
                let login_state = logged_in_resource.get();
                let items_state = items_resource.get();

                match login_state {
                    None => { view! {
                        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"checking if you are logged in"</p></div>
                    }.into_view()},
                    Some(Err(_)) => { view! {
                        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><meta http-equiv="refresh" content="0; url=/login"/></div>
                    }.into_view()},
                    Some(Ok(_)) => { view! {
                        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p></p></div>
                    }.into_view()},
                }
            }}
            {move || match logged_in_resource.get() {
                Some(Err(_)) => {
                    return view! {
                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><meta http-equiv="refresh" content="0; url=/login"/></div>
                }.into_view()},

                None => {return view! {
                <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"loading accounts..."</p></div>}.into_view()},

                Some(Ok(_)) => {
                    {items_resource.get().map(|result| {
                        match result {
                            Ok(items) => {
                                if items.len() < 3 {
                                    return view! {
                                        <div class="flex flex-col items-center text-center px-10 py-10"><p>"You must have three accounts in order to transact!"</p></div>
                                    }.into_view()
                                }
                                return view! {
                                    <div class="flex flex-col items-center text-center px-10 py-10">
                                    <h1 class="font-bold text-4xl">"Make a transaction"</h1>
                                    <p>"Please enter your values in cents"</p>
                                    <br/>
                                    <h2 class = "font-bold text-3xl">"Credit/Debit"</h2>
                                    <ActionForm action=update_action>

                                       <div class="flex items-center text-center px-10 py-10">

                                       <label class="block mb-2 font-medium">{items.get(0).unwrap().title.to_string()}</label>

                                       <div class="flex gap-4">

                                       <input
                                       type="hidden"
                                       name="id_one"
                                       value=items.get(0).unwrap().id
                                       />

                                       <input
                                       class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                       type="number"
                                       name="balance_add_cents_one"
                                       value=0
                                       />

                                       <input
                                       class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                       type="number"
                                       name="balance_remove_cents_one"
                                       value=0
                                       />
                                       </div>
                                       </div>

                                       <div class="flex items-center text-center px-10 py-10">
                                       <label>{items.get(1).unwrap().title.to_string()}</label>

                                       <br/>

                                       <div class="flex gap-4">
                                       <input
                                       type="hidden"
                                       name="id_two"
                                       value=items.get(1).unwrap().id
                                       />

                                       <input
                                       class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                       type="number"
                                       name="balance_add_cents_two"
                                       value=0
                                       />

                                       <input
                                       class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                       type="number"
                                       name="balance_remove_cents_two"
                                       value=0
                                       />

                                       </div>
                                       </div>

                                       <div class="flex items-center text-center px-10 py-10">
                                       <label class="block mb-2 font-medium">{items.get(2).unwrap().title.to_string()}</label>

                                       <br/>

                                       <div class="flex gap-4">
                                       <input
                                       type="hidden"
                                       name="id_three"
                                       value=items.get(2).unwrap().id
                                       />

                                       <input
                                       class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                       type="number"
                                       name="balance_add_cents_three"
                                       value=0
                                       />

                                       <input
                                       class = "shadow appearance-none border rounded py-2 px-2 text-gray-700 leading-tight focus:outline-none focus:shadow-outline"
                                       type="number"
                                       name="balance_remove_cents_three"
                                       value=0
                                       />

                                       </div>
                                       </div>

                                       <button class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400" type="submit">"Submit"</button>

                                       <br/>

                                   {move || match update_action.value().get() {
                                           None => {
                                               view! {
                                                   <div><p></p></div>
                                               }.into_view()
                                           }
                                           Some(Err(e)) => {
                                               view! {
                                                   <div><p>{e.to_string()}</p></div>
                                               }.into_view()
                                           }
                                           Some(Ok(val)) => {
                                               if val == TransactionResult::BALANCEMISMATCH {
                                                   view! {
                                                       <div><p>"Your debits do not equal your credits!"</p></div>
                                                   }.into_view()
                                               }
                                               else {
                                                   view! {
                                                       <div><p>"Updated successfully"</p></div>
                                                   }.into_view()
                                               }
                                           }
                                       }
                                   }

                                    </ActionForm>

                                    </div>
                                }.into_view()

                            }
                           Err(e) => return view! {<div class="flex flex-col items-center text-center px-10 py-10"><p>"Error: "{e.to_string()}</p></div>}.into_view()
                        }
                    })}
                }
            }.unwrap_or_else(|| view! {
                                        <div class="flex flex-col items-center text-center px-10 py-10">
                                            <p>"Loading transactions..."</p>
                                        </div>
                                    }.into_view())}

        </Suspense>
    }
}

#[component]
#[cfg(feature = "ssr")]
fn GeneralJournal() -> impl IntoView {
    use crate::api::{package_transactions, is_logged_in};
    use chrono::TimeZone;
    let transactions_resource = Resource::new(|| (), |_| async { package_transactions().await });
    let logged_in_resource = Resource::new(|| (), |_| async { is_logged_in().await });
    view! {

        <TopBar/>
        <Suspense fallback=|| view! { <p>"Loading transaction history..."</p> }>

        {move || {
            let login_state = logged_in_resource.get();
            match login_state {
                None => { view! {
                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"checking if you are logged in"</p></div>
                }.into_view()},
                Some(Err(_)) => { view! {
                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><meta http-equiv="refresh" content="0; url=/login"/></div>
                }.into_view()},
                Some(Ok(_)) => {
                    view! {
                    <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p></p></div>}.into_view()},
                }

        }}

        {move || transactions_resource.get().map(|transactions|{
               match transactions {
                   Ok(transactions) => {
                   if transactions.is_empty() {
                       return view! {
                           <div class="flex flex-col items-center text-center px-10 py-10"><p>"no transactions yet"</p></div>
                       }.into_view()
                   } else {
                   return view! {
                       <div class="flex flex-col items-center text-center px-10 py-10">
                            <h1 class="font-bold text-4xl">"Transactions"</h1>
                            <ul>
                                {transactions.iter().map(|packaged_transaction| {
                                     //using utc because we can't get users' timezones without JS
                                     view! {
                                    <div class="flex flex-col items-center text-center px-10 py-10">
                                        <li>
                                            <h2 font-bold text-xl>
                                            {chrono::Utc.timestamp(packaged_transaction.parent.created_at, 0).to_string()}":"
                                            </h2>
                                            <ul>
                                                {packaged_transaction.children.iter().map(|partial_transaction| {
                                                    view! {
                                                    <li>{partial_transaction.account_name.clone()} " : $" {partial_transaction.balance_diff_cents.abs()} " " {if partial_transaction.balance_diff_cents < 0 {"Dr".to_string()} else {"Cr".to_string()}} </li>
                                                    }
                                                }).collect_view()}
                                            </ul>
                                        </li>
                                    </div>
                                    }
                                }).collect_view()}
                            </ul>
                       </div>
                   }.into_view()}},
                   Err(e) => return view! {
                       <div class="flex flex-col items-center text-center px-10 py-10"><p>{e.to_string()}</p></div>
                   }.into_view(),
               }
            })}

        </Suspense>
    }
}
