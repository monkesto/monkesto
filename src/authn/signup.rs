use super::passkey::PasskeyId;
use super::user::{UserError, UserId};
use super::{AuthSession, AuthnService};
use crate::authn::corepasskey::CorePasskey;
use axum::extract::Extension;
use axum::extract::Form;
use axum::extract::Query;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::response::Response;
use maud::Markup;
use maud::PreEscaped;
use maud::html;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use webauthn_rs::prelude::PasskeyRegistration;
use webauthn_rs::prelude::RegisterPublicKeyCredential;
use webauthn_rs::prelude::Uuid;
use webauthn_rs::prelude::Webauthn;
use webauthn_rs_proto::AuthenticatorSelectionCriteria;
use webauthn_rs_proto::ResidentKeyRequirement;

use crate::authority::Actor;
use crate::authority::Authority;
use crate::email::Email;
use crate::monkesto_error::{MonkestoError, OrRedirect};
use crate::theme::theme_with_head;
use crate::time_provider::{DefaultTimeProvider, TimeProvider};

#[derive(Deserialize)]
pub struct SignupQuery {
    error: Option<String>,
    next: Option<String>,
}

pub async fn signup_get(
    Extension(webauthn_url): Extension<String>,
    query: Query<SignupQuery>,
) -> Markup {
    let error_message = match query.error.as_deref() {
        Some("email_taken") => {
            Some("Email is already registered. Please use another email address.")
        }
        Some("invalid_email") => Some("Invalid email format. Please enter a valid email address."),
        Some("session_expired") => Some("Your sign up session has expired. Please try again."),
        Some("registration_failed") => Some("Sign up failed. Please try again."),
        _ => None,
    };

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

                            @if let Some(ref next) = query.next {
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
                            @let signin_url = query.next.as_ref().map(|n| format!("signin?next={}", n)).unwrap_or_else(|| "signin".to_string());
                            a
                            href=(signin_url)
                            class="font-semibold text-indigo-600 hover:text-indigo-500 dark:text-indigo-400 dark:hover:text-indigo-300" {
                                "Sign in here"
                            }
                        }

                        @if let Some(error_str) = error_message {
                            div class="mt-6" {
                                p class="text-center text-sm/6 text-red-500" {
                                    (MonkestoError::decode(error_str))
                                }
                            }
                        }
                    }
                }
        },
    )
}

pub async fn signup_post(
    Extension(webauthn): Extension<Arc<Webauthn>>,
    Extension(authn_service): Extension<AuthnService>,
    Extension(webauthn_url): Extension<String>,
    mut auth_session: AuthSession,
    form: Form<HashMap<String, String>>,
) -> Result<Response, Redirect> {
    let next = form.get("next").cloned();
    const CALLBACK_URL: &str = "/signup";

    if let Some(_credential_json) = form.get("credential") {
        // handle credential submission

        let credential_json = form
            .get("credential")
            .map(|s| s.as_str())
            .ok_or(UserError::InvalidInput)
            .or_redirect(CALLBACK_URL)?;

        let credential: RegisterPublicKeyCredential = serde_json::from_str(credential_json)
            .map_err(UserError::from)
            .or_redirect(CALLBACK_URL)?;

        // Get registration state from session
        let session = &auth_session.session;
        let (email, user_id, webauthn_uuid, reg_state, stored_next) = session
            .get::<(String, UserId, Uuid, PasskeyRegistration, Option<String>)>("reg_state")
            .await
            .map_err(|e| UserError::Session(e.to_string()))
            .or_redirect(CALLBACK_URL)?
            .ok_or(UserError::SessionNotFound)
            .or_redirect(CALLBACK_URL)?;

        let next = next.or(stored_next);

        // Verify the registration
        match webauthn.finish_passkey_registration(&credential, &reg_state) {
            Ok(passkey) => {
                // Clear the registration state
                _ = session.remove_value("reg_state").await;

                // Generate a PasskeyId for this passkey
                let passkey_id = PasskeyId::new();

                // Store the new user and their passkey
                let email_validated = Email::try_new(&email).or_redirect(CALLBACK_URL)?;

                authn_service
                    .create_user(
                        user_id,
                        email_validated.clone(),
                        webauthn_uuid,
                        Authority::Direct(Actor::Anonymous),
                        DefaultTimeProvider.get_time(),
                    )
                    .await
                    .or_redirect(CALLBACK_URL)?;

                let ev_id = authn_service
                    .create_passkey(
                        passkey_id,
                        user_id,
                        CorePasskey(passkey),
                        Authority::Direct(Actor::User(user_id)),
                        DefaultTimeProvider.get_time(),
                    )
                    .await
                    .or_redirect(CALLBACK_URL)?;

                // Log in the newly registered user via axum_login
                let user = super::user::UserState {
                    id: user_id,
                    webauthn_uuid,
                    email: email_validated,
                };
                auth_session
                    .login(&user)
                    .await
                    .map_err(UserError::from)
                    .or_redirect(CALLBACK_URL)?;

                authn_service.wait_for(ev_id).await;

                let redirect_to = next.as_deref().unwrap_or("/journal");
                Ok(Redirect::to(redirect_to).into_response())
            }
            Err(_) => {
                // Clear the registration state on failure
                _ = session.remove_value("reg_state").await;

                Err(Redirect::to("/signup?error=registration_failed"))
            }
        }
    } else if let Some(email_str) = form.get("email") {
        // handle email submission
        let email = Email::try_new(email_str).or_redirect("/signup")?;

        let exclude_credentials = None;

        let user_id = UserId::new();

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
                    .await
                    .map_err(|e| UserError::SerdeJson(e.to_string()))
                    .or_redirect(CALLBACK_URL)?;

                let challenge_json = serde_json::to_string(&ccr)
                    .map_err(UserError::from)
                    .or_redirect("/signup")?;

                const SIGNUP_JS: &str = include_str!("signup.js");

                Ok(theme_with_head(
                    Some("Create Passkey"),
                    html! {
                    script
                        src="https://cdn.jsdelivr.net/npm/js-base64@3.7.4/base64.min.js"
                        crossorigin="anonymous" {}
                    meta name="webauthn_url" content=(webauthn_url);
                    script id="challenge-data" type="application/json" {
                        (PreEscaped(challenge_json))
                    }
                    script {
                           (PreEscaped(SIGNUP_JS))
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
            }).into_response())
            }
            Err(_) => Err(Redirect::to("/signup?error=registration_failed")),
        }
    } else {
        Err(UserError::InvalidInput).or_redirect("/signup")
    }
}
