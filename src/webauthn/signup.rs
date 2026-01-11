use axum::{
    extract::{Extension, Form, Query},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect},
};
use maud::{DOCTYPE, Markup, PreEscaped, html};
use serde::Deserialize;
use std::collections::HashMap;
use tower_sessions::Session;
use webauthn_rs::prelude::{PasskeyRegistration, RegisterPublicKeyCredential, Uuid};

use super::{error::WebauthnError, startup::WebauthnState};
use crate::maud_header::header;

#[derive(Deserialize)]
pub struct SignupQuery {
    error: Option<String>,
}

fn email_form_page(webauthn_url: &str, error_message: Option<&str>) -> Markup {
    header(html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Sign up - Monkesto" }
                meta name="webauthn_url" content=(webauthn_url);
            }
            body {
                div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8" {
                    div class="sm:mx-auto sm:w-full sm:max-w-sm" {
                        img src="/logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto";
                        h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white" {
                            "Sign up"
                        }
                    }

                    div class="mt-10 sm:mx-auto sm:w-full sm:max-w-sm" {
                        form method="POST" action="signup" class="space-y-6" {
                            div {
                                label
                                for="email"
                                class="block text-sm/6 font-medium text-gray-900 dark:text-gray-100" {
                                    "Email"
                                }
                                div class="mt-2" {
                                    input
                                    id="email"
                                    name="email"
                                    type="email"
                                    required
                                    class="block w-full rounded-md bg-white px-3 py-1.5 text-base text-gray-900 outline-1 -outline-offset-1 outline-gray-300 placeholder:text-gray-400 focus:outline-2 focus:-outline-offset-2 focus:outline-indigo-600 sm:text-sm/6 dark:bg-white/5 dark:text-white dark:outline-white/10 dark:placeholder:text-gray-500 dark:focus:outline-indigo-500";
                                }
                            }

                            div {
                                button
                                type="submit"
                                class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                                    "Continue"
                                }
                            }
                        }

                        p class="mt-10 text-center text-sm/6 text-gray-500 dark:text-gray-400" {
                            "Already have an account? "
                            a
                            href="signin"
                            class="font-semibold text-indigo-600 hover:text-indigo-500 dark:text-indigo-400 dark:hover:text-indigo-300" {
                                "Sign in here"
                            }
                        }

                        @if let Some(error_message) = error_message {
                            div class="mt-6" {
                                p class="text-center text-sm/6 text-red-500" {
                                    (error_message)
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

fn challenge_page(webauthn_url: &str, email: &str, challenge_data: &str) -> Markup {
    header(html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Create Passkey - Monkesto" }
                script
                    src="https://cdn.jsdelivr.net/npm/js-base64@3.7.4/base64.min.js"
                    crossorigin="anonymous" {}
                meta name="webauthn_url" content=(webauthn_url);
                script id="challenge-data" type="application/json" {
                    (PreEscaped(challenge_data))
                }
                script {
                    r#"
                    window.addEventListener('load', function() {
                        const challengeDataElement = document.getElementById('challenge-data');
                        if (!challengeDataElement) {
                            document.getElementById('flash_message').innerHTML = 'No challenge data available. Please try again.';
                            return;
                        }

                        let credentialCreationOptions;
                        try {
                            credentialCreationOptions = JSON.parse(challengeDataElement.textContent);
                        } catch (error) {
                            console.error('Failed to parse challenge data:', error);
                            document.getElementById('flash_message').innerHTML = 'Invalid challenge data. Please try again.';
                            return;
                        }

                        // Convert base64url strings to Uint8Arrays
                        credentialCreationOptions.publicKey.challenge = Base64.toUint8Array(
                            credentialCreationOptions.publicKey.challenge
                        );
                        credentialCreationOptions.publicKey.user.id = Base64.toUint8Array(
                            credentialCreationOptions.publicKey.user.id
                        );
                        credentialCreationOptions.publicKey.excludeCredentials?.forEach(function(listItem) {
                            listItem.id = Base64.toUint8Array(listItem.id);
                        });

                        // Show creating message
                        document.getElementById('status_message').innerHTML = 'Creating your passkey...';

                        navigator.credentials.create({
                            publicKey: credentialCreationOptions.publicKey
                        }).then(function(credential) {
                            // Convert response to base64url and submit via form
                            const credentialData = {
                                id: credential.id,
                                rawId: Base64.fromUint8Array(new Uint8Array(credential.rawId), true),
                                type: credential.type,
                                response: {
                                    attestationObject: Base64.fromUint8Array(
                                        new Uint8Array(credential.response.attestationObject), true
                                    ),
                                    clientDataJSON: Base64.fromUint8Array(
                                        new Uint8Array(credential.response.clientDataJSON), true
                                    )
                                }
                            };

                            document.getElementById('credential-field').value = JSON.stringify(credentialData);
                            document.getElementById('registration-form').submit();
                        }).catch(function(error) {
                            console.error('Registration error:', error);
                            document.getElementById('flash_message').innerHTML = 'Failed to create passkey: ' + error.message;
                        });
                    });
                    "#
                }
            }
            body {
                div class="flex min-h-full flex-col justify-center px-6 py-12 lg:px-8" {
                    div class="sm:mx-auto sm:w-full sm:max-w-sm" {
                        img src="/logo.svg" alt="Monkesto" class="mx-auto h-36 w-auto";
                        h2 class="mt-10 text-center text-2xl/9 font-bold tracking-tight text-gray-900 dark:text-white" {
                            "Create Your Passkey"
                        }
                        p class="mt-2 text-center text-sm/6 text-gray-600 dark:text-gray-400" {
                            "Email: " strong { (email) }
                        }
                    }

                    div class="mt-10 sm:mx-auto sm:w-full sm:max-w-sm" {
                        // Hidden form for credential submission
                        form id="registration-form" method="POST" action="signup" style="display: none;" {
                            input type="hidden" name="email" value=(email);
                            input type="hidden" id="credential-field" name="credential" value="";
                        }

                        div class="text-center" {
                            p id="status_message" class="text-lg text-gray-900 dark:text-white" {
                                "Please follow your device's prompts to create your passkey"
                            }

                            div class="mt-6" {
                                p id="flash_message" class="text-center text-sm/6 text-red-500" {}
                            }
                        }
                    }
                }
            }
        }
    })
}

async fn handle_signup_get(webauthn_url: String, query: Query<SignupQuery>) -> impl IntoResponse {
    // Handle error messages from query parameters
    let error_message = match query.error.as_deref() {
        Some("email_taken") => {
            Some("Email is already registered. Please use another email address.")
        }
        Some("invalid_email") => Some("Invalid email format. Please enter a valid email address."),
        Some("session_expired") => Some("Your sign up session has expired. Please try again."),
        Some("registration_failed") => Some("Sign up failed. Please try again."),
        _ => None,
    };

    let markup = email_form_page(&webauthn_url, error_message);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        markup,
    )
}

async fn handle_signup_post(
    app_state: WebauthnState,
    session: Session,
    webauthn_url: String,
    form_data: Form<HashMap<String, String>>,
) -> Result<axum::response::Response, WebauthnError> {
    // Check if this is an email submission or credential submission
    if let Some(_credential_json) = form_data.get("credential") {
        // This is step 2: credential submission
        handle_credential_submission(app_state, session, form_data).await
    } else if let Some(email) = form_data.get("email") {
        // This is step 1: email submission
        handle_email_submission(app_state, session, webauthn_url, email.clone()).await
    } else {
        Err(WebauthnError::InvalidInput)
    }
}

async fn handle_email_submission(
    app_state: WebauthnState,
    session: Session,
    webauthn_url: String,
    email: String,
) -> Result<axum::response::Response, WebauthnError> {
    // Validate email format (basic validation)
    if email.is_empty() || email.len() > 254 || !email.contains('@') || !email.contains('.') {
        return Ok(Redirect::to("/webauthn/signup?error=invalid_email").into_response());
    }

    // Check if email is already taken
    if app_state
        .storage
        .email_exists(&email)
        .await
        .unwrap_or(false)
    {
        return Ok(Redirect::to("/webauthn/signup?error=email_taken").into_response());
    }

    // Get existing credentials for exclusion
    let exclude_credentials = None; // New user, no existing credentials to exclude

    // Generate new user ID
    let user_unique_id = Uuid::new_v4();

    // Clear any previous registration state
    let _ = session.remove_value("reg_state").await;

    // Start passkey registration
    match app_state.webauthn.start_passkey_registration(
        user_unique_id,
        &email,
        &email,
        exclude_credentials,
    ) {
        Ok((ccr, reg_state)) => {
            // Store registration state in session
            session
                .insert("reg_state", (email.clone(), user_unique_id, reg_state))
                .await
                .map_err(|_| WebauthnError::Unknown)?;

            // Serialize challenge to JSON
            let challenge_json = serde_json::to_string(&ccr).map_err(|_| WebauthnError::Unknown)?;

            // Return challenge page
            let markup = challenge_page(&webauthn_url, &email, &challenge_json);
            Ok((
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/html")],
                markup,
            )
                .into_response())
        }
        Err(_) => Ok(Redirect::to("/webauthn/signup?error=registration_failed").into_response()),
    }
}

async fn handle_credential_submission(
    app_state: WebauthnState,
    session: Session,
    form_data: Form<HashMap<String, String>>,
) -> Result<axum::response::Response, WebauthnError> {
    // Extract credential from form
    let credential_json = form_data
        .get("credential")
        .ok_or(WebauthnError::InvalidInput)?;

    let credential: RegisterPublicKeyCredential =
        serde_json::from_str(credential_json).map_err(|_| WebauthnError::InvalidInput)?;

    // Get registration state from session
    let (email, user_unique_id, reg_state) = session
        .get::<(String, Uuid, PasskeyRegistration)>("reg_state")
        .await
        .map_err(|_| WebauthnError::Unknown)?
        .ok_or(WebauthnError::SessionExpired)?;

    // Verify the registration
    match app_state
        .webauthn
        .finish_passkey_registration(&credential, &reg_state)
    {
        Ok(passkey) => {
            // Clear the registration state
            let _ = session.remove_value("reg_state").await;

            // Store the new user and their passkey
            if app_state
                .storage
                .create_user(email.clone(), user_unique_id, passkey)
                .await
                .is_err()
            {
                return Ok(Redirect::to("/webauthn/signup?error=storage_error").into_response());
            }

            // Set authenticated session for the newly registered user
            session
                .insert("user_id", user_unique_id)
                .await
                .map_err(|_| WebauthnError::Unknown)?;

            // Redirect to passkey page
            Ok(Redirect::to("/webauthn/passkey").into_response())
        }
        Err(_) => {
            // Clear the registration state on failure
            let _ = session.remove_value("reg_state").await;

            Ok(Redirect::to("/webauthn/signup?error=registration_failed").into_response())
        }
    }
}

pub async fn signup_get(
    Extension(webauthn_url): Extension<String>,
    query: Query<SignupQuery>,
) -> impl IntoResponse {
    handle_signup_get(webauthn_url, query).await
}

pub async fn signup_post(
    Extension(app_state): Extension<WebauthnState>,
    Extension(webauthn_url): Extension<String>,
    session: Session,
    form: Form<HashMap<String, String>>,
) -> impl IntoResponse {
    match handle_signup_post(app_state, session, webauthn_url, form).await {
        Ok(response) => response,
        Err(error) => error.into_response(),
    }
}
