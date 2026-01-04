use axum::{
    http::{StatusCode, header},
    response::IntoResponse,
};
use maud::{DOCTYPE, Markup, html};
use std::env;

use crate::maud_header::header;

fn auth_page() -> Markup {
    header(html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Sign in - Monkesto" }
                script
                    src="https://cdn.jsdelivr.net/npm/js-base64@3.7.4/base64.min.js"
                    crossorigin="anonymous" {}
                script src="auth.js" async {}
                meta name="webauthn_url" content=(format!("{}/webauthn/", env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())));
            }
            body {
                div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8" {

                    div class="sm:mx-auto sm:w-full sm:max-w-sm" {
                        img src="/logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto";

                        h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white" {
                            "Sign in"
                        }
                    }

                    div class="mt-10 sm:mx-auto sm:w-full sm:max-w-sm" {

                        div class="space-y-6" {
                            div {
                                button
                                onclick="login()"
                                class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                                    "Sign in with Passkey"
                                }
                            }
                        }

                        p class="mt-10 text-center text-sm/6 text-gray-500 dark:text-gray-400" {
                            "Don't have an account? "
                            a
                            href="register"
                            class="font-semibold text-indigo-600 hover:text-indigo-500 dark:text-indigo-400 dark:hover:text-indigo-300" {
                                "Sign up here"
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

pub async fn login() -> impl IntoResponse {
    let markup = auth_page();
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        markup,
    )
}
