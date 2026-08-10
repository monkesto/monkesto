use super::user::DEV_USERS;
use super::user::{UserError, UserId};
use super::{AuthSession, AuthnService};
use crate::monkesto_error::{MonkestoError, OrRedirect};
use crate::theme::theme_with_head;
use axum::extract::Extension;
use axum::extract::Form;
use axum::extract::Query;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum_login::AuthnBackend;
use maud::html;
use maud::{Markup, PreEscaped};
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use webauthn_rs::prelude::PasskeyAuthentication;
use webauthn_rs::prelude::PublicKeyCredential;
use webauthn_rs::prelude::Webauthn;

#[derive(Deserialize)]
pub struct SigninQuery {
    err: Option<String>,
    next: Option<String>,
}

pub async fn signin_get(
    Extension(webauthn): Extension<Arc<Webauthn>>,
    Extension(authn_service): Extension<AuthnService>,
    Extension(webauthn_url): Extension<String>,
    auth_session: AuthSession,
    query: Query<SigninQuery>,
) -> Markup {
    // Clear any previous auth state
    let session = auth_session.session;
    _ = session.remove_value("identifierless_auth_state").await;

    // passing an empty slice to creds enables identifier-less discoverable credentials
    let challenge_data = match webauthn.start_passkey_authentication(&[]) {
        Ok((rcr, auth_state)) => {
            match session
                .insert("identifierless_auth_state", auth_state)
                .await
            {
                Ok(_) => serde_json::to_string(&rcr).ok(),
                Err(_) => None,
            }
        }
        Err(_) => None,
    };

    let error_str = query.err.clone().map(|str| {
        let error = MonkestoError::decode(&str);
        match error {
            MonkestoError::User(UserError::SessionNotFound) => {
                "Your authentication session has expired. Please try again.".to_string()
            }
            MonkestoError::User(UserError::AuthenticationFailed) => {
                "Authentication failed. Please try again.".to_string()
            }
            _ => error.to_string(),
        }
    });

    // Get dev users for the dev login form
    let dev_users = authn_service.get_dev_users().await;

    const SIGNIN_JS: &str = include_str!("signin.js");

    let next = query.next.as_deref();

    theme_with_head(
        Some("Sign in"),
        html! {
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
                (PreEscaped(SIGNIN_JS))
            }
        },
        html! {
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
                            @if let Some(next) = query.next.clone() {
                                input type="hidden" name="next" value=(next);
                            }
                        }

                        p class="mt-6 text-center text-sm/6 text-gray-500 dark:text-gray-400" {
                            "Don't have an account? "
                            @let signup_url = next.map(|n| format!("signup?next={}", n)).unwrap_or_else(|| "signup".to_string());
                            a
                            href=(signup_url)
                            class="font-semibold text-indigo-600 hover:text-indigo-500 dark:text-indigo-400 dark:hover:text-indigo-300" {
                                "Sign up here"
                            }
                        }

                        div class="mt-6" {
                            @if let Some(error_message) = error_str {
                                p id="flash_message" class="text-center text-sm/6 text-red-500" {
                                    (error_message)
                                }
                            } @else {
                                p id="flash_message" class="text-center text-sm/6 text-gray-500 dark:text-gray-400" {}
                            }
                        }

                        @if !dev_users.is_empty() {
                            div class="mt-10 border-t border-gray-200 dark:border-gray-700" {}
                            p style="margin-top: 1rem; margin-bottom: 1rem;" class="text-center text-xs text-gray-400 dark:text-gray-500" {
                                "Dev Login"
                            }
                            div class="space-y-2" {
                                @for user in dev_users {
                                    form method="POST" action="/signin" {
                                        input type="hidden" name="dev_user_id" value=(user.id.to_string());
                                        @if let Some(next) = next {
                                            input type="hidden" name="next" value=(next);
                                        }
                                        button
                                            type="submit"
                                            class="flex w-full justify-center rounded-md bg-gray-100 px-3 py-1.5 text-sm/6 font-medium text-gray-700 hover:bg-gray-200 dark:bg-gray-800 dark:text-gray-300 dark:hover:bg-gray-700" {
                                            (user.email.to_string())
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
        },
    )
}

pub async fn signin_post(
    Extension(webauthn): Extension<Arc<Webauthn>>,
    Extension(authn_service): Extension<AuthnService>,
    mut auth_session: AuthSession,
    form: Form<HashMap<String, String>>,
) -> Result<impl IntoResponse, Redirect> {
    const CALLBACK_URL: &str = "/signin";

    let next = form.get("next").cloned();

    // TODO(Gabriel): toggle dev login with an env variable
    if let Some(dev_user_id) = form.get("dev_user_id") {
        let user_id = UserId::from_str(dev_user_id).or_redirect(CALLBACK_URL)?;

        let user = authn_service
            .fetch_user(user_id)
            .await
            .or_redirect(CALLBACK_URL)?;

        if !DEV_USERS.clone().contains_key(&user.email) {
            return Err(UserError::InvalidInput).or_redirect(CALLBACK_URL);
        }

        auth_session
            .login(&user)
            .await
            .map_err(UserError::from)
            .or_redirect(CALLBACK_URL)?;

        let redirect_to = next.as_deref().unwrap_or("/journal");
        return Ok(Redirect::to(redirect_to).into_response());
    }

    let credential_json = form
        .get("credential")
        .ok_or(UserError::InvalidInput)
        .or_redirect(CALLBACK_URL)?;

    let credential: PublicKeyCredential = serde_json::from_str(credential_json)
        .map_err(UserError::from)
        .or_redirect(CALLBACK_URL)?;

    let session = &auth_session.session;
    let auth_state = session
        .get::<PasskeyAuthentication>("identifierless_auth_state")
        .await
        .map_err(UserError::from)
        .or_redirect(CALLBACK_URL)?
        .ok_or(MonkestoError::from(UserError::SessionNotFound).redirect(CALLBACK_URL))?;

    _ = session.remove_value("identifierless_auth_state").await;

    let auth_result = webauthn
        .finish_passkey_authentication(&credential, &auth_state)
        .map_err(|_| UserError::AuthenticationFailed)
        .or_redirect("/signin")?;

    let (user_id, _passkey_id) = authn_service
        .find_user_by_credential(auth_result.cred_id())
        .await
        .or_redirect("/signin")?
        .ok_or(UserError::AuthenticationFailed)
        .or_redirect("/signin")?;

    let user = authn_service
        .get_user(&user_id)
        .await
        .or_redirect(CALLBACK_URL)?
        .ok_or(MonkestoError::from(UserError::AuthenticationFailed))
        .or_redirect(CALLBACK_URL)?;

    auth_session
        .login(&user)
        .await
        .map_err(UserError::from)
        .or_redirect(CALLBACK_URL)?;

    let redirect_to = next.as_deref().unwrap_or("/journal");
    Ok(Redirect::to(redirect_to).into_response())
}
