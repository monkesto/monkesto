use super::CreateUser;
use super::user::get_user_id_from_session;
use crate::extensions;
use crate::known_errors::KnownErrors;
use crate::known_errors::return_error;
use axum::Extension;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::response::Response;
use leptos::prelude::*;
use maud::{DOCTYPE, html};
use sqlx::PgPool;
use tower_sessions::Session;

#[allow(dead_code)]
pub async fn client_login(
    Extension(pool): Extension<PgPool>,
    session: Session,
) -> impl IntoResponse {
    let session_id = match extensions::intialize_session(&session).await {
        Ok(s) => s,
        Err(e) => return return_error(e, "fetching session id"),
    };

    let logged_in = super::get_user_id(&session_id, &pool).await; // this throws an error if the database can't find an account associated with the session

    if logged_in.is_ok() {
        return Redirect::to("/").into_response();
    }

    let html = view! {
        <link rel="stylesheet" href="/pkg/monkesto.css" />
        <div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8">
            <div class="sm:mx-auto sm:w-full sm:max-w-sm">
                <img src="/logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto" />
                <h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white">
                    Sign in to your account
                </h2>
            </div>

            <div class="mt-10 sm:mx-auto sm:w-full sm:max-w-sm">
                // redirect to the homepage if the user's session id is already associated with an account
                <form action="/login" method="post">
                    <div class="space-y-6">
                        <div>
                            <label
                                for="username"
                                class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100"
                            >
                                Username
                            </label>
                            <div class="mt-2">
                                <input
                                    id="username"
                                    type="text"
                                    name="username"
                                    required
                                    autocomplete="username"
                                    class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                                />
                            </div>
                        </div>

                        <div>
                            <div class="flex items-center justify-between">
                                <label
                                    for="password"
                                    class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100"
                                >
                                    Password
                                </label>
                            // <div class="text-sm">
                            // <a
                            // href="#"
                            // class="font-semibold text-indigo-600 hover:text-indigo-500 dark:text-indigo-400 dark:hover:text-indigo-300"
                            // >
                            // Forgot password?
                            // </a>
                            // </div>
                            </div>
                            <div class="mt-2">
                                <input
                                    id="password"
                                    type="password"
                                    name="password"
                                    required
                                    autocomplete="current-password"
                                    class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                                />
                            </div>
                        </div>

                        <div>
                            <button
                                type="submit"
                                class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500"
                            >
                                Sign in
                            </button>
                        </div>
                    </div>
                </form>
                <p class="mt-10 text-center text-sm/6 text-gray-500 dark:text-gray-400">
                    Need an account?
                    <a
                        href="/signup"
                        class="font-semibold text-indigo-600 hover:text-indigo-500 dark:text-indigo-400 dark:hover:text-indigo-300"
                    >
                        Register
                    </a>
                </p>

            </div>
        </div>
    }.to_html();

    axum::response::Html(html).into_response()
}

// Redirect and html return different types, so they have to be converted into a response
// In the case that redirect isn't needed, markup can be returned directly
pub async fn client_login_maud(Extension(pool): Extension<PgPool>, session: Session) -> Response {
    let session_id = match extensions::intialize_session(&session).await {
        Ok(s) => s,
        Err(e) => return return_error(e, "fetching session id"),
    };

    let logged_in = super::get_user_id(&session_id, &pool).await; // this throws an error if the database can't find an account associated with the session

    if logged_in.is_ok() {
        return Redirect::to("/").into_response();
    }

    html! {
        (DOCTYPE)

        html class="h-full bg-white dark:bg-gray-900 text-gray-900 dark:text-white"{

            link rel="stylesheet" href="/pkg/monkesto.css";

            div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8" {

                div class="sm:mx-auto sm:w-full sm:max-w-sm" {
                    img src="/logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto";

                    h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white" {
                        "Sign in your account"
                    }
                }

                div class="mt-10 sm:mx-auto sm:w-full sm:max-w-sm" {

                    form action="/login" method="post" {
                        div class="space-y-6" {

                            label
                            for="username"
                            class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100" {
                                "Username"
                            }

                            div class="mt-2" {
                                input
                                id="username"
                                type="text"
                                name="username"
                                required
                                autocomplete="username"
                                class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                                ;
                            }
                        }

                        div {
                            div class="flex items-center justify-between" {
                                label
                                for="password"
                                class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100" {
                                    "Password"
                                }
                            }

                            div class="mt-2" {
                                input
                                id="password"
                                type="password"
                                name="password"
                                required
                                autocomplete="current-password"
                                class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                                ;
                            }
                        }

                        div {
                            button
                            type="submit"
                            class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                                "Sign in"
                            }
                        }
                    }

                    p class="mt-10 text-center text-sm/6 text-gray-500 dark:text-gray-400" {
                        "Need an account? "

                        a
                        href="/signup"
                        class="font-semibold text-indigo-600 hover:text-indigo-500 dark:text-indigo-400 dark:hover:text-indigo-300" {
                            "Register"
                        }
                    }
                }
            }
        }
    }.into_response()
}

#[component]
pub fn ClientSignUp() -> impl IntoView {
    let logged_in = Resource::new(|| (), |_| async { get_user_id_from_session().await }); // this throws an error if the database can't find an account associated with the session
    let signup = ServerAction::<CreateUser>::new();

    view! {
        <div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8">
            <div class="sm:mx-auto sm:w-full sm:max-w-sm">
                <img src="/logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto" />
                <h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white">
                    Sign Up
                </h2>
            </div>

            <div class="mt-10 sm:mx-auto sm:w-full sm:max-w-sm">
                <Suspense>

                    // redirect to the homepage if the user is already logged in
                    {Suspend::new(async move {
                        if logged_in.await.is_ok() {
                            view! { <meta http-equiv="refresh" content="0; url=/journal" /> }
                                .into_any()
                        } else {
                            view! { "" }.into_any()
                        }
                    })} <ActionForm action=signup>

                        <div class="space-y-6">
                            <div>
                                <label
                                    for="username"
                                    class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100"
                                >
                                    Username
                                </label>
                                <div class="mt-2">
                                    <input
                                        id="username"
                                        type="text"
                                        name="username"
                                        required
                                        class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                                    />
                                </div>
                            </div>

                            <div>
                                <div class="flex items-center justify-between">
                                    <label
                                        for="password"
                                        class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100"
                                    >
                                        Password
                                    </label>
                                </div>
                                <div class="mt-2">
                                    <input
                                        id="password"
                                        type="password"
                                        name="password"
                                        required
                                        class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                                    />
                                </div>
                            </div>
                            <div>
                                <div class="flex items-center justify-between">
                                    <label
                                        for="password"
                                        class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100"
                                    >
                                        Confirm Password
                                    </label>
                                </div>
                                <div class="mt-2">
                                    <input
                                        id="confirm_password"
                                        type="password"
                                        name="confirm_password"
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
                                    Register
                                </button>
                            </div>
                        </div>
                    </ActionForm>
                    <p class="mt-10 text-center text-sm/6 text-gray-500 dark:text-gray-400">
                        Already have an account?
                        <a
                            href="/login"
                            class="font-semibold text-indigo-600 hover:text-indigo-500 dark:text-indigo-400 dark:hover:text-indigo-300"
                        >
                            Sign In
                        </a>
                    </p>

                </Suspense>

                {match signup.value().get() {
                    Some(Err(e)) => {
                        if KnownErrors::parse_error(&e)
                            .is_some_and(|e| e == KnownErrors::SessionIdNotFound)
                        {
                            view! { <meta http-equiv="refresh" content="0; url=/login" /> }
                                .into_any()
                        } else {
                            view! {
                                "An error occurred while signing up: "
                                {e.to_string()}
                            }
                                .into_any()
                        }
                    }
                    _ => view! { "" }.into_any(),
                }}
            </div>
        </div>
    }
}
