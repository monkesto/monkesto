use axum::{
    extract::Extension,
    http::{StatusCode, header},
    response::IntoResponse,
};
use maud::{DOCTYPE, Markup, html};

use crate::maud_header::header;

fn auth_page(webauthn_url: &str) -> Markup {
    header(html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Register - Monkesto" }
                script
                    src="https://cdn.jsdelivr.net/npm/js-base64@3.7.4/base64.min.js"
                    crossorigin="anonymous" {}
                script src="auth.js" async {}
                meta name="webauthn_url" content=(webauthn_url);
            }
            body {
                div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8" {

                    div class="sm:mx-auto sm:w-full sm:max-w-sm" {
                        img src="/logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto";

                        h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white" {
                            "Register"
                        }
                    }

                    div class="mt-10 sm:mx-auto sm:w-full sm:max-w-sm" {

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
                                    placeholder="Enter your username"
                                    required
                                    class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500"
                                    ;
                                }
                            }

                            div {
                                button
                                onclick="register()"
                                class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                                    "Sign up with Passkey"
                                }
                            }
                        }

                        p class="mt-10 text-center text-sm/6 text-gray-500 dark:text-gray-400" {
                            "Already have an account? "
                            a
                            href="login"
                            class="font-semibold text-indigo-600 hover:text-indigo-500 dark:text-indigo-400 dark:hover:text-indigo-300" {
                                "Sign in here"
                            }
                        }

                        div class="mt-6" {
                            p id="flash_message" class="text-center text-sm/6 text-gray-500 dark:text-gray-400" {}
                        }
                    }
                }
            }
        }
    })
}

pub async fn register(Extension(webauthn_url): Extension<String>) -> impl IntoResponse {
    let markup = auth_page(&webauthn_url);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        markup,
    )
}
