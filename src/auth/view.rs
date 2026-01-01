use crate::extensions;
use crate::known_errors::UrlError;
use crate::maud_header;
use crate::{auth::get_user_id, known_errors::KnownErrors};
use axum::Extension;
use axum::extract::Query;
use axum::response::Redirect;
use maud::{Markup, html};
use sqlx::PgPool;
use tower_sessions::Session;

// Redirect and the html macro return different types, so they have to be converted into a response
// In the case that redirect isn't needed, Markup can be returned directly as it implements IntoResponse
pub async fn client_login(
    Extension(pool): Extension<PgPool>,
    session: Session,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    if let Ok(session_id) = extensions::intialize_session(&session).await
        && super::get_user_id(&session_id, &pool).await.is_ok()
    {
        return Err(Redirect::to("/"));
    }

    let username = err
        .err
        .as_ref()
        .and_then(|e| match KnownErrors::decode(e) {
            KnownErrors::LoginFailed { username } => Some(username),
            _ => None,
        })
        .unwrap_or_default();

    let content = html! {

        div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8" {

            div class="sm:mx-auto sm:w-full sm:max-w-sm" {
                img src="/logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto";

                h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white" {
                    "Sign in to your account"
                }
            }

            div class="mt-10 sm:mx-auto sm:w-full sm:max-w-sm" {

                form action="/login" method="post" {
                    div class="space-y-6" {

                        div {
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
                                value=(username)
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
                }

                p class="mt-10 text-center text-sm/6 text-gray-500 dark:text-gray-400" {
                    "Need an account? "

                    a
                    href="/signup"
                    class="font-semibold text-indigo-600 hover:text-indigo-500 dark:text-indigo-400 dark:hover:text-indigo-300" {
                        "Register"
                    }
                }

                @if let Some(e) = err.err {
                    p class="mt-10 text-center text-sm/6 text-gray-500 dark:text-gray-400" {
                        (format! ("error: {:?}", KnownErrors::decode(&e)))
                    }
                }
            }
        }
    };

    Ok(maud_header::header(content))
}

pub async fn client_signup(
    Extension(pool): Extension<PgPool>,
    session: Session,
    Query(err): Query<UrlError>,
) -> Result<Markup, Redirect> {
    if let Ok(session_id) = extensions::intialize_session(&session).await
        && get_user_id(&session_id, &pool).await.is_ok()
    {
        return Err(Redirect::to("/journal"));
    }

    let content = html! {
        div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8" {
            div class="sm:mx-auto sm:w-full sm:max-w-sm" {
                img src="/logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto";
                h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white" {
                    "Sign Up"
                }
            }

            div class="mt-10 sm:mx-auto sm:w-full sm:max-w-sm" {
                form action="/signup" method="post" {
                    div class="space-y-6" {
                        div {
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
                                    class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500";
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
                                    class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500";
                            }
                        }

                        div {
                            div class="flex items-center justify-between" {
                                label
                                    for="confirm_password"
                                    class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100" {
                                    "Confirm Password"
                                }
                            }
                            div class="mt-2" {
                                input
                                    id="confirm_password"
                                    type="password"
                                    name="confirm_password"
                                    required
                                    class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500";
                            }
                        }

                        div {
                            button
                                type="submit"
                                class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                                "Register"
                            }
                        }
                    }
                }

                p class="mt-10 text-center text-sm/6 text-gray-500 dark:text-gray-400" {
                    "Already have an account? "
                    a
                        href="/login"
                        class="font-semibold text-indigo-600 hover:text-indigo-500 dark:text-indigo-400 dark:hover:text-indigo-300" {
                        "Sign In"
                    }
                }

                @if let Some(e) = err.err {
                    p class="mt-10 text-center text-sm/6 text-gray-500 dark:text-gray-400" {
                        (format! ("error: {:?}", KnownErrors::decode(&e)))
                    }
                }
            }
        }
    };

    Ok(maud_header::header(content))
}
