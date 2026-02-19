use axum::extract::Extension;
use axum::extract::Form;
use axum::extract::Query;
use axum::http::header;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::response::Response;
use maud::html;
use maud::Markup;
use maud::PreEscaped;
use serde::Deserialize;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use thiserror::Error;
use webauthn_rs::prelude::AuthenticationResult;
use webauthn_rs::prelude::PasskeyAuthentication;
use webauthn_rs::prelude::PublicKeyCredential;
use webauthn_rs::prelude::RequestChallengeResponse;
use webauthn_rs::prelude::Webauthn;

use super::passkey::PasskeyStore;
use super::user::User;
use super::user::UserId;
use super::user::UserStore;
use super::AuthSession;
use crate::theme::theme_with_head;

/// Errors that occur during the signin flow.
#[derive(Error, Debug)]
pub enum SigninError {
    #[error("Authentication failed")]
    AuthenticationFailed,
    #[error("Authentication session expired")]
    SessionExpired,
    #[error("Invalid input data")]
    InvalidInput,
    #[error("Session error: {0}")]
    SessionError(#[from] tower_sessions::session::Error),
    #[error("User not found")]
    UserNotFound,
    #[error("Store operation failed: {0}")]
    StoreError(String),
    #[error("Login failed: {0}")]
    LoginFailed(String),
}

impl IntoResponse for SigninError {
    fn into_response(self) -> Response {
        match self {
            SigninError::SessionExpired => {
                Redirect::to("/signin?error=session_expired").into_response()
            }
            SigninError::AuthenticationFailed => {
                Redirect::to("/signin?error=auth_failed").into_response()
            }
            SigninError::InvalidInput => (StatusCode::BAD_REQUEST, "Invalid input").into_response(),
            SigninError::UserNotFound => (StatusCode::NOT_FOUND, "User not found").into_response(),
            SigninError::SessionError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Session error").into_response()
            }
            SigninError::StoreError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Store operation failed").into_response()
            }
            SigninError::LoginFailed(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Login failed").into_response()
            }
        }
    }
}

/// Handles WebAuthn authentication flow (signin).
/// This struct encapsulates the start and finish phases of authentication.
pub struct SigninAuthenticator<'a, P: PasskeyStore> {
    webauthn: &'a Webauthn,
    passkey_store: &'a P,
}

impl<'a, P: PasskeyStore> SigninAuthenticator<'a, P> {
    pub fn new(webauthn: &'a Webauthn, passkey_store: &'a P) -> Self {
        Self {
            webauthn,
            passkey_store,
        }
    }

    /// Start the authentication flow by loading credentials and generating a challenge.
    ///
    /// The allowCredentials list is cleared for a true identifier-less experience
    /// (the browser/OS will prompt the user to pick their passkey).
    ///
    /// Returns the challenge request and auth state, or None if it fails.
    pub async fn start(&self) -> Option<(RequestChallengeResponse, PasskeyAuthentication)> {
        let all_credentials = self
            .passkey_store
            .get_all_credentials()
            .await
            .unwrap_or_default();

        match self.webauthn.start_passkey_authentication(&all_credentials) {
            Ok((mut rcr, auth_state)) => {
                // Clear allowCredentials for true identifier-less experience
                rcr.public_key.allow_credentials.clear();
                Some((rcr, auth_state))
            }
            Err(_) => None,
        }
    }

    /// Finish the authentication flow by verifying the credential.
    ///
    /// Returns the user ID if authentication succeeds.
    pub async fn finish(
        &self,
        credential: &PublicKeyCredential,
        auth_state: &PasskeyAuthentication,
    ) -> Result<(UserId, AuthenticationResult), SigninError> {
        let auth_result = self
            .webauthn
            .finish_passkey_authentication(credential, auth_state)
            .map_err(|_| SigninError::AuthenticationFailed)?;

        let (user_id, _passkey_id) = self
            .passkey_store
            .find_user_by_credential(auth_result.cred_id().as_slice())
            .await
            .map_err(|e| SigninError::StoreError(e.to_string()))?
            .ok_or(SigninError::UserNotFound)?;

        Ok((user_id, auth_result))
    }
}

#[derive(Deserialize)]
pub struct SigninQuery {
    error: Option<String>,
    next: Option<String>,
}

fn auth_page(
    webauthn_url: &str,
    challenge_data: Option<&str>,
    error_message: Option<&str>,
    next: Option<&str>,
    dev_users: &[User],
) -> Markup {
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
                            @if let Some(next) = next {
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
                            @if let Some(error_message) = error_message {
                                p id="flash_message" class="text-center text-sm/6 text-red-500" {
                                    (error_message)
                                }
                            } @else {
                                p id="flash_message" class="text-center text-sm/6 text-gray-500 dark:text-gray-400" {}
                            }
                        }

                        // Dev login section (only shown if dev users exist)
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

async fn handle_signin_page<P: PasskeyStore>(
    webauthn: Arc<Webauthn>,
    passkey_store: Arc<P>,
    user_store: Arc<super::MemoryUserStore>,
    auth_session: AuthSession,
    webauthn_url: String,
    query: Query<SigninQuery>,
    next: Option<String>,
) -> impl IntoResponse {
    // Clear any previous auth state
    let session = auth_session.session;
    _ = session.remove_value("auth_state").await;
    _ = session.remove_value("usernameless_auth_state").await;

    // Generate challenge for identifier-less authentication (WebAuthn "usernameless")
    let authenticator = SigninAuthenticator::new(&webauthn, passkey_store.as_ref());
    let challenge_data = match authenticator.start().await {
        Some((rcr, auth_state)) => {
            // Store auth state in session
            match session
                .insert("identifierless_auth_state", auth_state)
                .await
            {
                Ok(_) => serde_json::to_string(&rcr).ok(),
                Err(_) => None,
            }
        }
        None => None,
    };

    let error_message: Option<&str> = None;

    // Handle error messages from query parameters
    let error_message = error_message.or_else(|| match query.error.as_deref() {
        Some("session_expired") => {
            Some("Your authentication session has expired. Please try again.")
        }
        Some("auth_failed") => Some("Authentication failed. Please try again."),
        _ => None,
    });

    // Get dev users for the dev login form
    let dev_users = user_store.get_dev_users().await;

    let markup = auth_page(
        &webauthn_url,
        challenge_data.as_deref(),
        error_message,
        next.as_deref(),
        &dev_users,
    );
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        markup,
    )
}

async fn handle_signin_completion<U: UserStore, P: PasskeyStore>(
    webauthn: Arc<Webauthn>,
    user_store: Arc<U>,
    passkey_store: Arc<P>,
    mut auth_session: AuthSession,
    form_data: Form<HashMap<String, String>>,
    next: Option<String>,
) -> Result<Response, SigninError> {
    // Extract credential from form
    let credential_json = form_data
        .get("credential")
        .ok_or(SigninError::InvalidInput)?;

    // Parse the JSON credential data
    let credential: PublicKeyCredential =
        serde_json::from_str(credential_json).map_err(|_| SigninError::InvalidInput)?;

    // Get auth state from session (checking both possible keys for compatibility)
    let session = &auth_session.session;
    let auth_state = session
        .get::<PasskeyAuthentication>("identifierless_auth_state")
        .await?
        .or_else(|| {
            // Try the regular auth_state key as fallback - this is sync so we can't await here
            // For now, just use the identifierless_auth_state
            None
        })
        .ok_or(SigninError::SessionExpired)?;

    // Verify the authentication using SigninAuthenticator
    let authenticator = SigninAuthenticator::new(&webauthn, passkey_store.as_ref());
    match authenticator.finish(&credential, &auth_state).await {
        Ok((user_id, _auth_result)) => {
            // Clear the auth state
            _ = session.remove_value("identifierless_auth_state").await;
            _ = session.remove_value("auth_state").await;

            // Get the user and log them in via axum_login
            let user = UserStore::get_user(user_store.deref(), user_id)
                .await
                .map_err(|e| SigninError::StoreError(e.to_string()))?
                .ok_or(SigninError::UserNotFound)?;

            auth_session
                .login(&user)
                .await
                .map_err(|e| SigninError::LoginFailed(e.to_string()))?;

            // Redirect to next or default
            let redirect_to = next.as_deref().unwrap_or("/journal");
            Ok(Redirect::to(redirect_to).into_response())
        }
        Err(_) => {
            // Clear the auth state on failure
            _ = session.remove_value("identifierless_auth_state").await;
            _ = session.remove_value("auth_state").await;

            // Redirect back to login with error
            Ok(Redirect::to("/signin?error=auth_failed").into_response())
        }
    }
}

pub async fn signin_get<P: PasskeyStore + 'static>(
    Extension(webauthn): Extension<Arc<Webauthn>>,
    Extension(passkey_store): Extension<Arc<P>>,
    Extension(user_store): Extension<Arc<super::MemoryUserStore>>,
    Extension(webauthn_url): Extension<String>,
    auth_session: AuthSession,
    query: Query<SigninQuery>,
) -> impl IntoResponse {
    let next = query.next.clone();
    handle_signin_page(
        webauthn,
        passkey_store,
        user_store,
        auth_session,
        webauthn_url,
        query,
        next,
    )
    .await
}

pub async fn signin_post<U: UserStore + 'static, P: PasskeyStore + 'static>(
    Extension(webauthn): Extension<Arc<Webauthn>>,
    Extension(user_store): Extension<Arc<U>>,
    Extension(passkey_store): Extension<Arc<P>>,
    auth_session: AuthSession,
    form: Form<HashMap<String, String>>,
) -> impl IntoResponse {
    let next = form.get("next").cloned();

    // Check for dev login first
    if let Some(dev_user_id) = form.get("dev_user_id") {
        return handle_dev_login(user_store, auth_session, dev_user_id, next).await;
    }

    match handle_signin_completion(
        webauthn,
        user_store,
        passkey_store,
        auth_session,
        form,
        next,
    )
    .await
    {
        Ok(response) => response.into_response(),
        Err(error) => error.into_response(),
    }
}

async fn handle_dev_login<U: UserStore>(
    user_store: Arc<U>,
    mut auth_session: AuthSession,
    dev_user_id: &str,
    next: Option<String>,
) -> Response {
    use super::user::UserId;
    use std::str::FromStr;

    // Parse the user ID
    let user_id = match UserId::from_str(dev_user_id) {
        Ok(id) => id,
        Err(_) => return Redirect::to("/signin?error=auth_failed").into_response(),
    };

    // Look up the user
    let user = match UserStore::get_user(user_store.deref(), user_id).await {
        Ok(Some(user)) => user,
        _ => return Redirect::to("/signin?error=auth_failed").into_response(),
    };

    // Verify this is a dev user
    if !super::MemoryUserStore::DEV_USERS.contains(&user.email.as_ref()) {
        return Redirect::to("/signin?error=auth_failed").into_response();
    }

    // Log them in
    if auth_session.login(&user).await.is_err() {
        return Redirect::to("/signin?error=auth_failed").into_response();
    }

    // Redirect to next or default
    let redirect_to = next.as_deref().unwrap_or("/journal");
    Redirect::to(redirect_to).into_response()
}
