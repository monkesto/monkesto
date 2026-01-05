use axum::{
    extract::{Extension, Form, Query},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect},
};
use maud::{DOCTYPE, Markup, PreEscaped, html};
use serde::Deserialize;
use std::collections::HashMap;
use tower_sessions::Session;
use webauthn_rs::prelude::{PasskeyAuthentication, PublicKeyCredential};

use super::{error::WebauthnError, startup::AppState};
use crate::maud_header::header;

#[derive(Deserialize)]
pub struct LoginQuery {
    error: Option<String>,
}

fn auth_page(
    webauthn_url: &str,
    challenge_data: Option<&str>,
    error_message: Option<&str>,
) -> Markup {
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
                meta name="webauthn_url" content=(webauthn_url);
                @if let Some(challenge_data) = challenge_data {
                    script id="challenge-data" type="application/json" {
                        (PreEscaped(challenge_data))
                    }
                }
                script {
                    r#"
                    function login() {
                        const challengeDataElement = document.getElementById('challenge-data');
                        if (!challengeDataElement) {
                            document.getElementById('flash_message').innerHTML = 'No challenge data available. Please refresh the page.';
                            return;
                        }

                        let credentialRequestOptions;
                        try {
                            credentialRequestOptions = JSON.parse(challengeDataElement.textContent);
                        } catch (error) {
                            console.error('Failed to parse challenge data:', error);
                            document.getElementById('flash_message').innerHTML = 'Invalid challenge data. Please refresh the page.';
                            return;
                        }

                        // Convert base64url strings to Uint8Arrays
                        credentialRequestOptions.publicKey.challenge = Base64.toUint8Array(
                            credentialRequestOptions.publicKey.challenge
                        );
                        credentialRequestOptions.publicKey.allowCredentials?.forEach(function(listItem) {
                            listItem.id = Base64.toUint8Array(listItem.id);
                        });

                        navigator.credentials.get({
                            publicKey: credentialRequestOptions.publicKey
                        }).then(function(assertion) {
                            // Convert response to base64url and submit via form
                            const credentialData = {
                                id: assertion.id,
                                rawId: Base64.fromUint8Array(new Uint8Array(assertion.rawId), true),
                                type: assertion.type,
                                response: {
                                    authenticatorData: Base64.fromUint8Array(new Uint8Array(assertion.response.authenticatorData), true),
                                    clientDataJSON: Base64.fromUint8Array(new Uint8Array(assertion.response.clientDataJSON), true),
                                    signature: Base64.fromUint8Array(new Uint8Array(assertion.response.signature), true),
                                    userHandle: Base64.fromUint8Array(new Uint8Array(assertion.response.userHandle), true)
                                }
                            };

                            document.getElementById('credential-field').value = JSON.stringify(credentialData);
                            document.getElementById('auth-form').submit();
                        }).catch(function(error) {
                            console.error('Authentication error:', error);
                            document.getElementById('flash_message').innerHTML = 'Authentication failed: ' + error.message;
                        });
                    }
                    "#
                }
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

                        // Hidden form for credential submission
                        form id="auth-form" method="POST" action="login" style="display: none;" {
                            input type="hidden" id="credential-field" name="credential" value="";
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
                            @if let Some(error_message) = error_message {
                                p id="flash_message" class="text-center text-sm/6 text-red-500" {
                                    (error_message)
                                }
                            } @else {
                                p id="flash_message" class="text-center text-sm/6 text-gray-500 dark:text-gray-400" {}
                            }
                        }
                    }
                }
            }
        }
    })
}

async fn handle_login_page(
    app_state: AppState,
    session: Session,
    webauthn_url: String,
    query: Query<LoginQuery>,
) -> impl IntoResponse {
    // Clear any previous auth state
    let _ = session.remove_value("auth_state").await;
    let _ = session.remove_value("usernameless_auth_state").await;

    // For usernameless authentication, load all credentials
    let users_guard = app_state.users.lock().await;
    let all_credentials: Vec<_> = users_guard.keys.values().flatten().cloned().collect();
    drop(users_guard);

    let (challenge_data, error_message) = if all_credentials.is_empty() {
        // No credentials available
        (
            None,
            Some("No registered users found. Please register first."),
        )
    } else {
        // Generate challenge for usernameless authentication
        match app_state
            .webauthn
            .start_passkey_authentication(&all_credentials)
        {
            Ok((mut rcr, auth_state)) => {
                // Clear allowCredentials for true usernameless experience
                rcr.public_key.allow_credentials.clear();

                // Store auth state in session
                match session.insert("usernameless_auth_state", auth_state).await {
                    Ok(_) => {
                        // Serialize challenge to JSON
                        match serde_json::to_string(&rcr) {
                            Ok(json) => (Some(json), None),
                            Err(_) => (
                                None,
                                Some("Failed to generate challenge. Please try again."),
                            ),
                        }
                    }
                    Err(_) => (None, Some("Session error. Please try again.")),
                }
            }
            Err(_) => (
                None,
                Some("Failed to generate authentication challenge. Please try again."),
            ),
        }
    };

    // Handle error messages from query parameters
    let error_message = error_message.or_else(|| match query.error.as_deref() {
        Some("session_expired") => {
            Some("Your authentication session has expired. Please try again.")
        }
        Some("auth_failed") => Some("Authentication failed. Please try again."),
        _ => None,
    });

    let markup = auth_page(&webauthn_url, challenge_data.as_deref(), error_message);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        markup,
    )
}

async fn handle_login_completion(
    app_state: AppState,
    session: Session,
    form_data: Form<HashMap<String, String>>,
) -> Result<impl IntoResponse, WebauthnError> {
    // Extract credential from form
    let credential_json = form_data
        .get("credential")
        .ok_or(WebauthnError::InvalidInput)?;

    // Parse the JSON credential data
    let credential: PublicKeyCredential =
        serde_json::from_str(credential_json).map_err(|_| WebauthnError::InvalidInput)?;

    // Get auth state from session (checking both possible keys for compatibility)
    let auth_state = session
        .get::<PasskeyAuthentication>("usernameless_auth_state")
        .await
        .map_err(|_| WebauthnError::Unknown)?
        .or_else(|| {
            // Try the regular auth_state key as fallback - this is sync so we can't await here
            // For now, just use the usernameless_auth_state
            None
        })
        .ok_or(WebauthnError::SessionExpired)?;

    // Verify the authentication
    match app_state
        .webauthn
        .finish_passkey_authentication(&credential, &auth_state)
    {
        Ok(auth_result) => {
            // Clear the auth state
            let _ = session.remove_value("usernameless_auth_state").await;
            let _ = session.remove_value("auth_state").await;

            // Find which user this credential belongs to
            let users_guard = app_state.users.lock().await;
            let _user_unique_id = users_guard
                .keys
                .iter()
                .find_map(|(uid, keys)| {
                    if keys
                        .iter()
                        .any(|key| key.cred_id() == auth_result.cred_id())
                    {
                        Some(*uid)
                    } else {
                        None
                    }
                })
                .ok_or(WebauthnError::UserNotFound)?;
            drop(users_guard);

            // Set authenticated session (you'll need to implement your session management)
            // session.insert("user_id", user_unique_id).await?;

            // Return simple success page
            Ok((
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/html")],
                "<!DOCTYPE html><html><head><title>Success</title></head><body><h1>Successfully logged in!</h1></body></html>",
            ).into_response())
        }
        Err(_) => {
            // Clear the auth state on failure
            let _ = session.remove_value("usernameless_auth_state").await;
            let _ = session.remove_value("auth_state").await;

            // Redirect back to login with error
            Ok(Redirect::to("/webauthn/login?error=auth_failed").into_response())
        }
    }
}

pub async fn login_get(
    Extension(app_state): Extension<AppState>,
    Extension(webauthn_url): Extension<String>,
    session: Session,
    query: Query<LoginQuery>,
) -> impl IntoResponse {
    handle_login_page(app_state, session, webauthn_url, query).await
}

pub async fn login_post(
    Extension(app_state): Extension<AppState>,
    session: Session,
    form: Form<HashMap<String, String>>,
) -> impl IntoResponse {
    match handle_login_completion(app_state, session, form).await {
        Ok(response) => response.into_response(),
        Err(error) => error.into_response(),
    }
}
