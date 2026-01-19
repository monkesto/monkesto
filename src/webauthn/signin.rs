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

use std::sync::Arc;
use webauthn_rs::prelude::Webauthn;

use super::error::WebauthnError;
use super::storage::PasskeyStore;
use crate::maud_header::header;

#[derive(Deserialize)]
pub struct SigninQuery {
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
                    function signin() {
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
                                onclick="signin()"
                                class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                                    "Sign in with Passkey"
                                }
                            }
                        }

                        // Hidden form for credential submission
                        form id="auth-form" method="POST" action="signin" style="display: none;" {
                            input type="hidden" id="credential-field" name="credential" value="";
                        }

                        p class="mt-10 text-center text-sm/6 text-gray-500 dark:text-gray-400" {
                            "Don't have an account? "
                            a
                            href="signup"
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

async fn handle_signin_page<P: PasskeyStore>(
    webauthn: Arc<Webauthn>,
    passkey_store: Arc<P>,
    session: Session,
    webauthn_url: String,
    query: Query<SigninQuery>,
) -> impl IntoResponse {
    // Clear any previous auth state
    let _ = session.remove_value("auth_state").await;
    let _ = session.remove_value("usernameless_auth_state").await;

    // For identifier-less authentication (WebAuthn terminology: "usernameless"), load all credentials
    let all_credentials = passkey_store
        .get_all_credentials()
        .await
        .unwrap_or_default();

    let (challenge_data, error_message) = if all_credentials.is_empty() {
        // No credentials available
        (
            None,
            Some("No registered users found. Please register first."),
        )
    } else {
        // Generate challenge for identifier-less authentication (WebAuthn "usernameless")
        match webauthn.start_passkey_authentication(&all_credentials) {
            Ok((mut rcr, auth_state)) => {
                // Clear allowCredentials for true identifier-less experience
                rcr.public_key.allow_credentials.clear();

                // Store auth state in session
                match session
                    .insert("identifierless_auth_state", auth_state)
                    .await
                {
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

async fn handle_signin_completion<P: PasskeyStore>(
    webauthn: Arc<Webauthn>,
    passkey_store: Arc<P>,
    session: Session,
    form_data: Form<HashMap<String, String>>,
) -> Result<axum::response::Response, WebauthnError> {
    // Extract credential from form
    let credential_json = form_data
        .get("credential")
        .ok_or(WebauthnError::InvalidInput)?;

    // Parse the JSON credential data
    let credential: PublicKeyCredential =
        serde_json::from_str(credential_json).map_err(|_| WebauthnError::InvalidInput)?;

    // Get auth state from session (checking both possible keys for compatibility)
    let auth_state = session
        .get::<PasskeyAuthentication>("identifierless_auth_state")
        .await
        .map_err(|_| WebauthnError::Unknown)?
        .or_else(|| {
            // Try the regular auth_state key as fallback - this is sync so we can't await here
            // For now, just use the identifierless_auth_state
            None
        })
        .ok_or(WebauthnError::SessionExpired)?;

    // Verify the authentication
    match webauthn.finish_passkey_authentication(&credential, &auth_state) {
        Ok(auth_result) => {
            // Clear the auth state
            let _ = session.remove_value("identifierless_auth_state").await;
            let _ = session.remove_value("auth_state").await;

            // Find which user this credential belongs to
            let (user_id, _passkey_id) = passkey_store
                .find_user_by_credential(auth_result.cred_id().as_slice())
                .await
                .map_err(|_| WebauthnError::Unknown)?
                .ok_or(WebauthnError::UserNotFound)?;

            // Set authenticated session
            session
                .insert("user_id", user_id)
                .await
                .map_err(|_| WebauthnError::Unknown)?;

            // Redirect to passkey page
            Ok(Redirect::to("/webauthn/passkey").into_response())
        }
        Err(_) => {
            // Clear the auth state on failure
            let _ = session.remove_value("identifierless_auth_state").await;
            let _ = session.remove_value("auth_state").await;

            // Redirect back to login with error
            Ok(Redirect::to("/webauthn/signin?error=auth_failed").into_response())
        }
    }
}

pub async fn signin_get<P: PasskeyStore + 'static>(
    Extension(webauthn): Extension<Arc<Webauthn>>,
    Extension(passkey_store): Extension<Arc<P>>,
    Extension(webauthn_url): Extension<String>,
    session: Session,
    query: Query<SigninQuery>,
) -> impl IntoResponse {
    handle_signin_page(webauthn, passkey_store, session, webauthn_url, query).await
}

pub async fn signin_post<P: PasskeyStore + 'static>(
    Extension(webauthn): Extension<Arc<Webauthn>>,
    Extension(passkey_store): Extension<Arc<P>>,
    session: Session,
    form: Form<HashMap<String, String>>,
) -> impl IntoResponse {
    match handle_signin_completion(webauthn, passkey_store, session, form).await {
        Ok(response) => response.into_response(),
        Err(error) => error.into_response(),
    }
}
