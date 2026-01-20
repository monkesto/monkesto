use axum::{
    extract::Extension,
    http::{StatusCode, header},
    response::IntoResponse,
};
use maud::{DOCTYPE, Markup, html};
use std::sync::Arc;

use super::AuthSession;
use super::passkey::{Passkey, PasskeyStore};
use super::user::UserStore;
use crate::maud_header::header;

fn me_page(email: &str, passkeys: &[Passkey]) -> Markup {
    header(html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Profile - Monkesto" }
            }
            body {
                div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8" {
                    // Sign out button at the very top
                    div class="sm:mx-auto sm:w-full sm:max-w-sm mb-8" {
                        form method="POST" action="signout" {
                            button
                            type="submit"
                            class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                                "Sign out"
                            }
                        }
                    }

                    div class="sm:mx-auto sm:w-full sm:max-w-sm" {
                        img src="/logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto";

                        h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white" {
                            "Profile"
                        }
                    }

                    div class="mt-10 sm:mx-auto sm:w-full sm:max-w-sm" {
                        div class="bg-white dark:bg-gray-800 rounded-lg shadow p-6 space-y-4" {
                            div {
                                h3 class="text-lg font-medium text-gray-900 dark:text-white" {
                                    "Your Account"
                                }
                                p class="text-sm text-gray-600 dark:text-gray-400" {
                                    (email)
                                }
                            }

                            div {
                                h4 class="text-md font-medium text-gray-900 dark:text-white mb-3" {
                                    "Registered Passkeys"
                                }

                                @if passkeys.is_empty() {
                                    p class="text-sm text-gray-500 dark:text-gray-400" {
                                        "No passkeys registered"
                                    }
                                } @else {
                                    div class="space-y-2" {
                                        @for (index, stored) in passkeys.iter().enumerate() {
                                            div class="border border-gray-200 dark:border-gray-600 rounded p-3" {
                                                div class="flex justify-between items-start" {
                                                    div {
                                                        p class="text-sm font-medium text-gray-900 dark:text-white" {
                                                            "Passkey " (index + 1)
                                                        }
                                                        p class="text-xs text-gray-500 dark:text-gray-400 font-mono" {
                                                            (stored.id.to_string())
                                                        }
                                                    }
                                                    div {
                                                        form method="POST" action=(format!("passkey/{}/delete", stored.id)) style="display: inline;" {
                                                            button
                                                            type="submit"
                                                            onclick="return confirm('Are you sure you want to delete this passkey?')"
                                                            class="text-xs px-2 py-1 bg-red-600 text-white rounded hover:bg-red-500 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-1" {
                                                                "Delete"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            // Add new passkey button (below all passkeys)
                            div class="mt-4 pt-4 border-t border-gray-200 dark:border-gray-600" {
                                form method="POST" action="passkey" {
                                    button
                                    type="submit"
                                    class="flex w-full justify-center rounded-md bg-green-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-green-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-green-600 dark:bg-green-500 dark:shadow-none dark:hover:bg-green-400 dark:focus-visible:outline-green-500" {
                                        "Add New Passkey"
                                    }
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

pub async fn me_get<U: UserStore + 'static, P: PasskeyStore + 'static>(
    Extension(user_store): Extension<Arc<U>>,
    Extension(passkey_store): Extension<Arc<P>>,
    auth_session: AuthSession,
) -> impl IntoResponse {
    // Check if user is logged in
    let user_id = match auth_session.user {
        Some(ref user) => user.id,
        None => {
            // Not logged in
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/html")],
                not_logged_in_page(),
            );
        }
    };

    // Get user passkeys
    let passkeys = passkey_store
        .get_user_passkeys(&user_id)
        .await
        .unwrap_or_default();

    // Get the email for this user
    let email = user_store
        .get_user_email(&user_id)
        .await
        .unwrap_or_else(|_| "unknown@example.com".to_string());

    let markup = me_page(&email, &passkeys);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        markup,
    )
}
