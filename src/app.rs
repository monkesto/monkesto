#[cfg(feature = "ssr")]
use crate::api::{get_accounts, AddAccount};
use leptos::prelude::*;

#[cfg(feature = "ssr")]
use leptos_axum::redirect;

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
                <Routes fallback=|| "Page not found.".into_view()>
                    <Route path=StaticSegment("") view=HomePage/>
                    <Route path=StaticSegment("/transact") view=Transact/>
                </Routes>
            </main>
        </Router>
    }
}

/// Renders the home page of your application.
#[cfg(feature = "ssr")]
#[component]
fn HomePage() -> impl IntoView {
    view! {
        <head>
        <AccountList/>
        <AddAccount/>
        </head>
    }
}

#[cfg(feature = "ssr")]
#[component]
fn AccountList() -> impl IntoView {
    let accounts = Resource::new(|| (), |_| async { get_accounts().await });

    view! {
        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4">
            <h1 class="font-bold text-4xl">"Accounts"</h1>
        </div>
        <Suspense>
        {move || match accounts.get() {
            None => view! { <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"Loading accounts..."</p></div> }.into_view(),
            Some(Err(e)) => { redirect("/"); // Hack around the fact that the browser needs to re-fetch the session after initializing it.
                                             // A better alternative needs to be found because the page may refresh indefinitely if the user's browser rejects the cookie.
                return view! {
                <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"Error loading accounts: " {e.to_string()}</p></div>}.into_view()},
            Some(Ok(s)) => {
                if s.is_empty() {
                    view! {
                        <div class="mx-auto flex min-w-full flex-col items-center px-4 py-4"><p>"No accounts yet. Add one below!"</p></div>
                    }.into_view()
                    } else {
                        view! {
                            <div class="mx-auto flex min-w-full flex-col items-center">
                                <ul>
                                    {s.into_iter()
                                        .map(|n| view! { <li class = "px-1 py-1 font-bold text-2xl">{n.title}"     "{format!("{}${}.{:02}", if n.balance_cents < 0 {"-"} else {""}, (n.balance_cents.abs() / 100), ((n.balance_cents).abs() % 100))}</li>})
                                        .collect_view()}
                                </ul>
                                <a href="/transact" class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400">"make a transaction"</a>
                            </div>
                        }.into_view()
                    }
                }
            }
        }
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
    use crate::{api::Transact, types::TransactionResult};

    let items_resource = Resource::new(|| (), |_| async { get_accounts().await });
    let update_action = ServerAction::<Transact>::new();

    view! {
        <head>
        <Suspense fallback=|| view! { <p>"Loading..."</p> }>
             {move || items_resource.get().map(|result| {
                 match result {
                     Ok(items) => {
                         if items.len() < 3 {
                             return view! {
                                 <div class="flex flex-col items-center text-center px-10 py-10"><p>"You must have three accounts in order to transact!"</p></div>
                             }.into_view()
                         }
                         view! {
                             <div class="flex flex-col items-center text-center px-10 py-10">
                             <a href = "/" class="mt-3 rounded bg-purple-900 px-2 py-2 font-bold text-white hover:bg-blue-400">homepage</a>
                             <h1 class="font-bold text-4xl">"make a transaction"</h1>
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
        </Suspense>
        </head>
    }
}
