use axum::{
    extract::Form,
    http::{StatusCode, header},
    response::{IntoResponse, Redirect},
};
use maud::{DOCTYPE, Markup, html};

use std::collections::HashMap;
use tower_sessions::Session;

use super::error::WebauthnError;
use crate::maud_header::header;

fn signout_page(message: Option<&str>) -> Markup {
    header(html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Sign out - Monkesto" }
            }
            body {
                div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8" {
                    div class="sm:mx-auto sm:w-full sm:max-w-sm" {
                        img src="/logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto";

                        h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white" {
                            "Sign out"
                        }

                        @if let Some(msg) = message {
                            p class="mt-4 text-center text-sm text-gray-600 dark:text-gray-400" {
                                (msg)
                            }
                        } @else {
                            p class="mt-4 text-center text-sm text-gray-600 dark:text-gray-400" {
                                "Are you sure you want to sign out?"
                            }
                        }
                    }

                    div class="mt-10 sm:mx-auto sm:w-full sm:max-w-sm" {
                        div class="space-y-4" {
                            form method="POST" action="signout" {
                                button
                                type="submit"
                                class="flex w-full justify-center rounded-md bg-red-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-red-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-red-600 dark:bg-red-500 dark:shadow-none dark:hover:bg-red-400 dark:focus-visible:outline-red-500" {
                                    "Yes, sign out"
                                }
                            }

                            a
                            href="whoami"
                            class="flex w-full justify-center rounded-md bg-gray-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-gray-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-gray-600 dark:bg-gray-500 dark:shadow-none dark:hover:bg-gray-400 dark:focus-visible:outline-gray-500" {
                                "Cancel"
                            }
                        }
                    }
                }
            }
        }
    })
}

pub async fn signout_get() -> impl IntoResponse {
    let markup = signout_page(None);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        markup,
    )
}

pub async fn signout_post(
    session: Session,
    _form: Form<HashMap<String, String>>,
) -> Result<impl IntoResponse, WebauthnError> {
    // Clear the user session
    let _ = session.remove_value("user_id").await;

    // Clear any other auth-related session data
    let _ = session.remove_value("identifierless_auth_state").await;
    let _ = session.remove_value("auth_state").await;
    let _ = session.remove_value("reg_state").await;

    // Redirect to sign in page
    Ok(Redirect::to("/webauthn/signin").into_response())
}
