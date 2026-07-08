use axum::extract::Extension;
use axum::extract::Form;
use axum::extract::Query;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::response::Response;
use maud::Markup;
use maud::PreEscaped;
use maud::html;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;
use webauthn_rs::prelude::PasskeyRegistration;
use webauthn_rs::prelude::RegisterPublicKeyCredential;
use webauthn_rs::prelude::Uuid;
use webauthn_rs::prelude::Webauthn;
use webauthn_rs_proto::AuthenticatorSelectionCriteria;
use webauthn_rs_proto::ResidentKeyRequirement;

use super::passkey::{CorePasskey, CreatePasskey, PasskeyId};
use super::user::{CreateUser, UserId};
use super::{AuthInterface, AuthSession};

use crate::authority::Actor;
use crate::authority::Authority;
use crate::email::Email;
use crate::theme::theme_with_head;
use crate::time_provider::{DefaultTimeProvider, TimeProvider};

/// Errors that occur during the signup flow.
#[derive(Error, Debug)]
pub enum SignupError {
    #[error("Session expired")]
    SessionExpired,
    #[error("Invalid input data")]
    InvalidInput,
    #[error("Session error: {0}")]
    SessionError(#[from] tower_sessions::session::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("Login failed: {0}")]
    LoginFailed(String),
}

impl IntoResponse for SignupError {
    fn into_response(self) -> Response {
        match self {
            SignupError::SessionExpired => {
                Redirect::to("/signup?error=session_expired").into_response()
            }
            SignupError::InvalidInput => (StatusCode::BAD_REQUEST, "Invalid input").into_response(),
            SignupError::SessionError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Session error").into_response()
            }
            SignupError::SerializationError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Serialization error").into_response()
            }
            SignupError::LoginFailed(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Login failed").into_response()
            }
        }
    }
}

#[derive(Deserialize)]
pub struct SignupQuery {
    error: Option<String>,
    next: Option<String>,
}

fn email_form_page(webauthn_url: &str, error_message: Option<&str>, next: Option<&str>) -> Markup {
    theme_with_head(
        Some("Sign up"),
        html! {
            meta name="webauthn_url" content=(webauthn_url);
        },
        html! {
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

                            @if let Some(next) = next {
                                input type="hidden" name="next" value=(next);
                            }

                            div {
                                button
                                type="submit"
                                class="flex w-full justify-center rounded-md bg-indigo-600 px-3 py-1.5 text-sm/6 font-semibold text-white shadow-xs hover:bg-indigo-500 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-indigo-600 dark:bg-indigo-500 dark:shadow-none dark:hover:bg-indigo-400 dark:focus-visible:outline-indigo-500" {
                                    "Continue"
                                }
                            }
                        }

                        p class="mt-6 text-center text-sm/6 text-gray-500 dark:text-gray-400" {
                            "Already have an account? "
                            @let signin_url = next.map(|n| format!("signin?next={}", n)).unwrap_or_else(|| "signin".to_string());
                            a
                            href=(signin_url)
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
        },
    )
}

fn challenge_page(
    webauthn_url: &str,
    email: &str,
    challenge_data: &str,
    next: Option<&str>,
) -> Markup {
    theme_with_head(
        Some("Create Passkey"),
        html! {
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
        },
        html! {
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
                            @if let Some(next) = next {
                                input type="hidden" name="next" value=(next);
                            }
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
        },
    )
}

async fn handle_signup_get(
    webauthn_url: String,
    query: Query<SignupQuery>,
    next: Option<String>,
) -> impl IntoResponse {
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

    let markup = email_form_page(&webauthn_url, error_message, next.as_deref());
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        markup,
    )
}

async fn handle_email_submission(
    webauthn: Arc<Webauthn>,
    auth_interface: AuthInterface,
    auth_session: AuthSession,
    webauthn_url: String,
    email: Email,
    next: Option<String>,
) -> Result<Response, SignupError> {
    // Check if email is already taken
    if auth_interface.email_exists(&email).await.unwrap_or(false) {
        return Ok(Redirect::to("/signup?error=email_taken").into_response());
    }

    // Get existing credentials for exclusion
    let exclude_credentials = None; // New user, no existing credentials to exclude

    // Generate new user ID (our internal identifier)
    let user_id = UserId::new();

    // Generate webauthn UUID (for webauthn-rs compatibility)
    let webauthn_uuid = Uuid::new_v4();

    // Clear any previous registration state
    let session = &auth_session.session;
    _ = session.remove_value("reg_state").await;

    // Start passkey registration
    match webauthn.start_passkey_registration(
        webauthn_uuid,
        email.as_ref(),
        email.as_ref(),
        exclude_credentials,
    ) {
        Ok((mut ccr, reg_state)) => {
            ccr.public_key.authenticator_selection = Some(AuthenticatorSelectionCriteria {
                authenticator_attachment: None,
                resident_key: Some(ResidentKeyRequirement::Required),
                require_resident_key: true,
                user_verification: webauthn_rs_proto::UserVerificationPolicy::Required,
            });

            // Store registration state in session (including next for the credential submission step)
            session
                .insert(
                    "reg_state",
                    (
                        email.clone(),
                        user_id,
                        webauthn_uuid,
                        reg_state,
                        next.clone(),
                    ),
                )
                .await?;

            // Serialize challenge to JSON
            let challenge_json = serde_json::to_string(&ccr)?;

            // Return challenge page
            let markup = challenge_page(
                &webauthn_url,
                email.as_ref(),
                &challenge_json,
                next.as_deref(),
            );
            Ok((
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/html")],
                markup,
            )
                .into_response())
        }
        Err(_) => Ok(Redirect::to("/signup?error=registration_failed").into_response()),
    }
}

async fn handle_credential_submission(
    webauthn: Arc<Webauthn>,
    auth_interface: AuthInterface,
    mut auth_session: AuthSession,
    form_data: Form<HashMap<String, String>>,
    next: Option<String>,
) -> Result<Response, SignupError> {
    // Extract credential from form
    let credential_json = form_data
        .get("credential")
        .ok_or(SignupError::InvalidInput)?;

    let credential: RegisterPublicKeyCredential =
        serde_json::from_str(credential_json).map_err(|_| SignupError::InvalidInput)?;

    // Get registration state from session
    let session = &auth_session.session;
    let (email, user_id, webauthn_uuid, reg_state, stored_next) = session
        .get::<(String, UserId, Uuid, PasskeyRegistration, Option<String>)>("reg_state")
        .await?
        .ok_or(SignupError::SessionExpired)?;

    // Use next from form if provided, otherwise fall back to stored next
    let next = next.or(stored_next);

    // Verify the registration
    match webauthn.finish_passkey_registration(&credential, &reg_state) {
        Ok(passkey) => {
            // Clear the registration state
            _ = session.remove_value("reg_state").await;

            // Generate a PasskeyId for this passkey
            let passkey_id = PasskeyId::new();

            // Store the new user and their passkey
            let email_validated = Email::try_new(&email).map_err(|_| SignupError::InvalidInput)?;

            auth_interface
                .decision_maker
                .make(CreateUser::new(
                    user_id,
                    email_validated.clone(),
                    webauthn_uuid,
                    Authority::Direct(Actor::Anonymous),
                    DefaultTimeProvider.get_time(),
                ))
                .await
                .map_err(|e| SignupError::LoginFailed(e.to_string()))?;

            auth_interface
                .decision_maker
                .make(CreatePasskey::new(
                    passkey_id,
                    user_id,
                    CorePasskey(passkey),
                    Authority::Direct(Actor::User(user_id)),
                    DefaultTimeProvider.get_time(),
                ))
                .await
                .map_err(|e| SignupError::LoginFailed(e.to_string()))?;

            // Log in the newly registered user via axum_login
            let user = super::user::UserState {
                id: user_id,
                webauthn_uuid,
                email: email_validated,
            };
            auth_session
                .login(&user)
                .await
                .map_err(|e| SignupError::LoginFailed(e.to_string()))?;

            // Redirect to next or default
            let redirect_to = next.as_deref().unwrap_or("/journal");
            Ok(Redirect::to(redirect_to).into_response())
        }
        Err(_) => {
            // Clear the registration state on failure
            _ = session.remove_value("reg_state").await;

            Ok(Redirect::to("/signup?error=registration_failed").into_response())
        }
    }
}

pub async fn signup_get(
    Extension(webauthn_url): Extension<String>,
    query: Query<SignupQuery>,
) -> impl IntoResponse {
    let next = query.next.clone();
    handle_signup_get(webauthn_url, query, next).await
}

pub async fn signup_post(
    Extension(webauthn): Extension<Arc<Webauthn>>,
    Extension(auth_interface): Extension<AuthInterface>,
    Extension(webauthn_url): Extension<String>,
    auth_session: AuthSession,
    form: Form<HashMap<String, String>>,
) -> impl IntoResponse {
    let next = form.get("next").cloned();
    if let Some(_credential_json) = form.get("credential") {
        handle_credential_submission(webauthn, auth_interface, auth_session, form, next).await
    } else if let Some(email_str) = form.get("email") {
        let email = match Email::try_new(email_str) {
            Ok(em) => em,
            Err(_) => return Err(SignupError::InvalidInput),
        };

        handle_email_submission(
            webauthn,
            auth_interface,
            auth_session,
            webauthn_url,
            email,
            next,
        )
        .await
    } else {
        Err(SignupError::InvalidInput)
    }
}
