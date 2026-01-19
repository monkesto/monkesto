use axum::{
    extract::{Extension, Form, Path},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect},
};
use maud::{DOCTYPE, Markup, html};

use std::collections::HashMap;
use tower_sessions::Session;
use webauthn_rs::prelude::{PasskeyRegistration, RegisterPublicKeyCredential};

use std::sync::Arc;
use webauthn_rs::prelude::Webauthn;

use super::authority::Authority;
use super::error::WebauthnError;
use super::user::{UserId, UserStore};
use crate::id;
use crate::ident::Ident;
use crate::known_errors::KnownErrors;
use crate::maud_header::header;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display},
    ops::Deref,
    str::FromStr,
};

fn passkeys_page(email: &str, passkeys: &[Passkey]) -> Markup {
    header(html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Passkeys - Monkesto" }
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
                            "Passkeys"
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

pub async fn passkey_get<U: UserStore + 'static, P: PasskeyStore + 'static>(
    Extension(user_store): Extension<Arc<U>>,
    Extension(passkey_store): Extension<Arc<P>>,
    session: Session,
) -> impl IntoResponse {
    // Check if user is logged in
    let user_id = match session.get::<UserId>("user_id").await {
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

    let markup = passkeys_page(&email, &passkeys);
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html")],
        markup,
    )
}

pub async fn delete_passkey_post<P: PasskeyStore + 'static>(
    Extension(passkey_store): Extension<Arc<P>>,
    session: Session,
    Path(passkey_id_str): Path<String>,
) -> Result<impl IntoResponse, WebauthnError> {
    // Check if user is logged in
    let user_id = session
        .get::<UserId>("user_id")
        .await
        .map_err(|_| WebauthnError::Unknown)?
        .ok_or(WebauthnError::SessionExpired)?;

    // Parse the PasskeyId
    let passkey_id = passkey_id_str
        .parse::<PasskeyId>()
        .map_err(|_| WebauthnError::InvalidInput)?;

    // Remove the passkey from the user's passkeys (only if it belongs to them)
    match passkey_store.remove_passkey(&user_id, &passkey_id).await {
        Ok(true) => {
            // Passkey was successfully removed
        }
        Ok(false) => {
            // Passkey wasn't found for this user
            return Err(WebauthnError::UserNotFound);
        }
        Err(_) => {
            // Storage error
            return Err(WebauthnError::Unknown);
        }
    }

    // Redirect back to passkey page
    Ok(Redirect::to("/webauthn/passkey").into_response())
}

pub async fn create_passkey_post<U: UserStore + 'static, P: PasskeyStore + 'static>(
    Extension(webauthn): Extension<Arc<Webauthn>>,
    Extension(user_store): Extension<Arc<U>>,
    Extension(passkey_store): Extension<Arc<P>>,
    session: Session,
    form: Form<HashMap<String, String>>,
) -> Result<impl IntoResponse, WebauthnError> {
    // Check if user is logged in
    let user_id = session
        .get::<UserId>("user_id")
        .await
        .map_err(|_| WebauthnError::Unknown)?
        .ok_or(WebauthnError::SessionExpired)?;

    // Check if this is a credential submission or initial request
    if let Some(credential_json) = form.get("credential") {
        // This is credential submission - finish registration
        let credential: RegisterPublicKeyCredential =
            serde_json::from_str(credential_json).map_err(|_| WebauthnError::InvalidInput)?;

        // Get registration state from session
        let reg_state = session
            .get::<PasskeyRegistration>("add_passkey_reg_state")
            .await
            .map_err(|_| WebauthnError::Unknown)?
            .ok_or(WebauthnError::SessionExpired)?;

        // Verify the registration
        match webauthn.finish_passkey_registration(&credential, &reg_state) {
            Ok(passkey) => {
                // Clear the registration state
                let _ = session.remove_value("add_passkey_reg_state").await;

                // Generate a PasskeyId for this passkey
                let passkey_id = PasskeyId::new();

                // Add the new passkey to the user's existing passkeys
                if passkey_store
                    .add_passkey(&user_id, passkey_id, passkey)
                    .await
                    .is_err()
                {
                    return Ok(
                        Redirect::to("/webauthn/passkey?error=storage_error").into_response()
                    );
                }

                // Redirect back to passkey management page
                Ok(Redirect::to("/webauthn/passkey").into_response())
            }
            Err(_) => {
                // Clear the registration state on failure
                let _ = session.remove_value("add_passkey_reg_state").await;
                Ok(Redirect::to("/webauthn/passkey?error=registration_failed").into_response())
            }
        }
    } else {
        // This is initial request - start registration
        // Get user's existing passkeys
        let existing_passkeys = passkey_store
            .get_user_passkeys(&user_id)
            .await
            .unwrap_or_default();

        // Get user's email
        let email = user_store
            .get_user_email(&user_id)
            .await
            .unwrap_or_else(|_| "unknown@example.com".to_string());

        // Get the webauthn UUID for this user
        let webauthn_uuid = user_store
            .get_webauthn_uuid(&user_id)
            .await
            .map_err(|_| WebauthnError::Unknown)?;

        let exclude_credentials: Vec<_> = existing_passkeys
            .iter()
            .map(|stored| stored.passkey.cred_id().clone())
            .collect();

        // Clear any previous registration state
        let _ = session.remove_value("add_passkey_reg_state").await;

        // Start passkey registration
        match webauthn.start_passkey_registration(
            webauthn_uuid,
            &email,
            &email,
            Some(exclude_credentials),
        ) {
            Ok((ccr, reg_state)) => {
                // Store registration state in session
                session
                    .insert("add_passkey_reg_state", reg_state)
                    .await
                    .map_err(|_| WebauthnError::Unknown)?;

                // Serialize challenge to JSON
                let challenge_json =
                    serde_json::to_string(&ccr).map_err(|_| WebauthnError::Unknown)?;

                // Return challenge page
                let markup = add_passkey_challenge_page(&email, &challenge_json);
                Ok((
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "text/html")],
                    markup,
                )
                    .into_response())
            }
            Err(_) => {
                Ok(Redirect::to("/webauthn/passkey?error=registration_failed").into_response())
            }
        }
    }
}

fn add_passkey_challenge_page(email: &str, challenge_data: &str) -> maud::Markup {
    use maud::{PreEscaped, html};

    header(html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { "Add New Passkey - Monkesto" }
                script
                    src="https://cdn.jsdelivr.net/npm/js-base64@3.7.4/base64.min.js"
                    crossorigin="anonymous" {}
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
                        document.getElementById('status_message').innerHTML = 'Creating your new passkey...';

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
                            "Add New Passkey"
                        }
                        p class="mt-2 text-center text-sm/6 text-gray-600 dark:text-gray-400" {
                            "Email: " strong { (email) }
                        }
                    }

                    div class="mt-10 sm:mx-auto sm:w-full sm:max-w-sm" {
                        // Hidden form for credential submission
                        form id="registration-form" method="POST" action="passkey" style="display: none;" {
                            input type="hidden" id="credential-field" name="credential" value="";
                        }

                        div class="text-center" {
                            p id="status_message" class="text-lg text-gray-900 dark:text-white" {
                                "Please follow your device's prompts to create your new passkey"
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

id!(PasskeyId, Ident::new16());

#[derive(Debug, Clone)]
pub struct Passkey {
    pub id: PasskeyId,
    pub passkey: webauthn_rs::prelude::Passkey,
}

#[expect(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PasskeyEvent {
    Created { id: PasskeyId, by: Authority },
    Deleted { id: PasskeyId, by: Authority },
}

#[derive(Debug, thiserror::Error)]
pub enum PasskeyStoreError {
    #[error("Storage operation failed: {0}")]
    #[allow(dead_code)]
    OperationFailed(String),
}

#[async_trait::async_trait]
pub trait PasskeyStore: Send + Sync {
    type EventId: Send + Sync + Clone;
    type Error;

    // async fn record(event: PasskeyEvent) -> Result<Self::EventId, Self::Error>;

    /// Get all passkeys for a specific user
    async fn get_user_passkeys(&self, user_id: &UserId) -> Result<Vec<Passkey>, Self::Error>;

    /// Add a new passkey to an existing user
    async fn add_passkey(
        &self,
        user_id: &UserId,
        passkey_id: PasskeyId,
        passkey: webauthn_rs::prelude::Passkey,
    ) -> Result<(), Self::Error>;

    /// Remove a specific passkey from a user by PasskeyId
    async fn remove_passkey(
        &self,
        user_id: &UserId,
        passkey_id: &PasskeyId,
    ) -> Result<bool, Self::Error>;

    /// Get all credentials from all users (for usernameless authentication)
    async fn get_all_credentials(&self) -> Result<Vec<webauthn_rs::prelude::Passkey>, Self::Error>;

    /// Find UserId and PasskeyId by passkey credential ID
    async fn find_user_by_credential(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<(UserId, PasskeyId)>, Self::Error>;
}

use tokio::sync::Mutex;

struct PasskeyData {
    keys: HashMap<UserId, Vec<Passkey>>,
}

impl PasskeyData {
    fn new() -> Self {
        Self {
            keys: HashMap::new(),
        }
    }
}

/// In-memory storage implementation for passkeys using HashMap
pub struct MemoryPasskeyStore {
    data: Arc<Mutex<PasskeyData>>,
}

impl MemoryPasskeyStore {
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(PasskeyData::new())),
        }
    }
}

impl Default for MemoryPasskeyStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl PasskeyStore for MemoryPasskeyStore {
    type EventId = ();
    type Error = PasskeyStoreError;

    async fn get_user_passkeys(&self, user_id: &UserId) -> Result<Vec<Passkey>, PasskeyStoreError> {
        let data = self.data.lock().await;
        Ok(data.keys.get(user_id).cloned().unwrap_or_default())
    }

    async fn add_passkey(
        &self,
        user_id: &UserId,
        passkey_id: PasskeyId,
        passkey: webauthn_rs::prelude::Passkey,
    ) -> Result<(), PasskeyStoreError> {
        let mut data = self.data.lock().await;

        // Create entry if user doesn't exist in passkey store yet
        let passkeys = data.keys.entry(*user_id).or_insert_with(Vec::new);
        passkeys.push(Passkey {
            id: passkey_id,
            passkey,
        });

        Ok(())
    }

    async fn remove_passkey(
        &self,
        user_id: &UserId,
        passkey_id: &PasskeyId,
    ) -> Result<bool, PasskeyStoreError> {
        let mut data = self.data.lock().await;

        match data.keys.get_mut(user_id) {
            Some(passkeys) => {
                let initial_len = passkeys.len();
                passkeys.retain(|stored| &stored.id != passkey_id);
                Ok(passkeys.len() < initial_len)
            }
            None => Ok(false), // User has no passkeys, so nothing was removed
        }
    }

    async fn get_all_credentials(
        &self,
    ) -> Result<Vec<webauthn_rs::prelude::Passkey>, PasskeyStoreError> {
        let data = self.data.lock().await;
        let credentials = data
            .keys
            .values()
            .flatten()
            .map(|stored| stored.passkey.clone())
            .collect();
        Ok(credentials)
    }

    async fn find_user_by_credential(
        &self,
        credential_id: &[u8],
    ) -> Result<Option<(UserId, PasskeyId)>, PasskeyStoreError> {
        let data = self.data.lock().await;

        for (user_id, passkeys) in &data.keys {
            for stored in passkeys {
                if stored.passkey.cred_id().as_slice() == credential_id {
                    return Ok(Some((*user_id, stored.id)));
                }
            }
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_passkey_store_operations() {
        let passkey_store = Arc::new(MemoryPasskeyStore::new());
        let user_id = UserId::new();

        // Initially user should have no passkeys
        assert!(
            passkey_store
                .get_user_passkeys(&user_id)
                .await
                .expect("Should get user passkeys")
                .is_empty()
        );

        // Removing non-existent passkey should return false
        let passkey_id = PasskeyId::new();
        assert!(
            !passkey_store
                .remove_passkey(&user_id, &passkey_id)
                .await
                .expect("Should remove passkey")
        );

        // Test that get_all_credentials works when empty
        assert!(
            passkey_store
                .get_all_credentials()
                .await
                .expect("Should get all credentials")
                .is_empty()
        );

        // Test that find_user_by_credential returns None when empty
        assert!(
            passkey_store
                .find_user_by_credential(&[1, 2, 3, 4])
                .await
                .expect("Should find user by credential")
                .is_none()
        );
    }
}
