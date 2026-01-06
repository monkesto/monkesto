use axum::{
    extract::Extension,
    http::{StatusCode, header},
    response::IntoResponse,
};
use maud::{DOCTYPE, Markup, html};
use tower_sessions::Session;
use webauthn_rs::prelude::Uuid;

use super::startup::AppState;
use crate::maud_header::header;

fn whoami_page(email: &str, user_id: &Uuid) -> Markup {
    header(html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Who am I? - Monkesto" }
            }
            body {
                div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8" {
                    div class="sm:mx-auto sm:w-full sm:max-w-sm" {
                        img src="/logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto";

                        h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white" {
                            "Who am I?"
                        }
                    }

                    div class="mt-10 sm:mx-auto sm:w-full sm:max-w-sm" {
                        div class="bg-white dark:bg-gray-800 rounded-lg shadow p-6 space-y-4" {
                            div {
                                h3 class="text-lg font-medium text-gray-900 dark:text-white" {
                                    "Your Account Information"
                                }
                            }

                            div class="space-y-2" {
                                div {
                                    span class="text-sm font-medium text-gray-500 dark:text-gray-400" {
                                        "Email: "
                                    }
                                    span class="text-sm text-gray-900 dark:text-white" {
                                        (email)
                                    }
                                }

                                div {
                                    span class="text-sm font-medium text-gray-500 dark:text-gray-400" {
                                        "User ID: "
                                    }
                                    span class="text-sm font-mono text-gray-900 dark:text-white" {
                                        (user_id)
                                    }
                                }
                            }
                        }

                        div class="mt-6" {
                            form method="POST" action="signout" {
                                button
                                type="submit"
                                class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                                    "Sign out"
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

fn not_logged_in_page() -> Markup {
    header(html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Not Logged In - Monkesto" }
            }
            body {
                div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8" {
                    div class="sm:mx-auto sm:w-full sm:max-w-sm" {
                        img src="/logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto";

                        h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white" {
                            "Not Logged In"
                        }

                        p class="mt-4 text-center text-sm text-gray-600 dark:text-gray-400" {
                            "You need to sign in to view this page."
                        }

                        div class="mt-6" {
                            a
                            href="signin"
                            class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                                "Sign In"
                            }
                        }
                    }
                }
            }
        }
    })
}

pub async fn whoami_get(
    Extension(app_state): Extension<AppState>,
    session: Session,
) -> impl IntoResponse {
    // Check if user is logged in
    let user_id = match session.get::<Uuid>("user_id").await {
        Ok(Some(id)) => id,
        Ok(None) | Err(_) => {
            // Not logged in
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/html")],
                not_logged_in_page(),
            );
        }
    };

    // Get user information
    let users_guard = app_state.users.lock().await;

    // Find the email for this user
    let email = users_guard
        .email_to_id
        .iter()
        .find_map(|(email, id)| if *id == user_id { Some(email) } else { None })
        .cloned()
        .unwrap_or_else(|| "unknown@example.com".to_string());

    drop(users_guard);

    let markup = whoami_page(&email, &user_id);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        markup,
    )
}
