use axum::{
    extract::{Extension, Form, Path},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
};
use thiserror::Error;

use std::collections::HashMap;
use std::sync::Arc;
use webauthn_rs::prelude::{PasskeyRegistration, RegisterPublicKeyCredential, Webauthn};

use super::AuthSession;
use super::layout::layout;
use super::user::{UserId, UserStore};
use crate::authority::{Actor, Authority};
use crate::event::EventStore;
use crate::id;
use crate::ident::Ident;
use crate::known_errors::KnownErrors;
use maud::{PreEscaped, html};

/// Errors that occur during passkey management operations.
#[derive(Error, Debug)]
pub enum PasskeyError {
    #[error("Session expired")]
    SessionExpired,
    #[error("Invalid input data")]
    InvalidInput,
    #[error("Session error: {0}")]
    SessionError(#[from] tower_sessions::session::Error),
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("Store operation failed: {0}")]
    StoreError(String),
}

impl IntoResponse for PasskeyError {
    fn into_response(self) -> Response {
        match self {
            PasskeyError::SessionExpired => {
                Redirect::to("/signin?error=session_expired").into_response()
            }
            PasskeyError::InvalidInput => {
                (StatusCode::BAD_REQUEST, "Invalid input").into_response()
            }
            PasskeyError::SessionError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Session error").into_response()
            }
            PasskeyError::SerializationError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Serialization error").into_response()
            }
            PasskeyError::StoreError(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Store operation failed").into_response()
            }
        }
    }
}

use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Display},
    ops::Deref,
    str::FromStr,
};

pub async fn delete_passkey_post<P: PasskeyStore + 'static>(
    Extension(passkey_store): Extension<Arc<P>>,
    auth_session: AuthSession,
    Path(passkey_id_str): Path<String>,
) -> Result<impl IntoResponse, PasskeyError> {
    // Check if user is logged in
    let user_id = auth_session
        .user
        .as_ref()
        .map(|u| u.id)
        .ok_or(PasskeyError::SessionExpired)?;

    // Parse the PasskeyId
    let passkey_id = passkey_id_str
        .parse::<PasskeyId>()
        .map_err(|_| PasskeyError::InvalidInput)?;

    // Remove the passkey from the user's passkeys
    passkey_store
        .record(
            passkey_id,
            Authority::Direct(Actor::User(user_id)),
            PasskeyEvent::Deleted { user_id },
        )
        .await
        .map_err(|e| PasskeyError::StoreError(e.to_string()))?;

    // Redirect back to passkey page
    Ok(Redirect::to("/me").into_response())
}

pub async fn create_passkey_post<U: UserStore + 'static, P: PasskeyStore + 'static>(
    Extension(webauthn): Extension<Arc<Webauthn>>,
    Extension(user_store): Extension<Arc<U>>,
    Extension(passkey_store): Extension<Arc<P>>,
    auth_session: AuthSession,
    form: Form<HashMap<String, String>>,
) -> Result<impl IntoResponse, PasskeyError> {
    // Check if user is logged in
    let user_id = auth_session
        .user
        .as_ref()
        .map(|u| u.id)
        .ok_or(PasskeyError::SessionExpired)?;

    let session = &auth_session.session;

    // Check if this is a credential submission or initial request
    if let Some(credential_json) = form.get("credential") {
        // This is credential submission - finish registration
        let credential: RegisterPublicKeyCredential =
            serde_json::from_str(credential_json).map_err(|_| PasskeyError::InvalidInput)?;

        // Get registration state from session
        let reg_state = session
            .get::<PasskeyRegistration>("add_passkey_reg_state")
            .await?
            .ok_or(PasskeyError::SessionExpired)?;

        // Verify the registration
        match webauthn.finish_passkey_registration(&credential, &reg_state) {
            Ok(passkey) => {
                // Clear the registration state
                let _ = session.remove_value("add_passkey_reg_state").await;

                // Generate a PasskeyId for this passkey
                let passkey_id = PasskeyId::new();

                // Add the new passkey to the user's existing passkeys
                if passkey_store
                    .record(
                        passkey_id,
                        Authority::Direct(Actor::User(user_id)),
                        PasskeyEvent::Created { user_id, passkey },
                    )
                    .await
                    .is_err()
                {
                    return Ok(Redirect::to("/me?error=storage_error").into_response());
                }

                // Redirect back to passkey management page
                Ok(Redirect::to("/me").into_response())
            }
            Err(_) => {
                // Clear the registration state on failure
                let _ = session.remove_value("add_passkey_reg_state").await;
                Ok(Redirect::to("/me?error=registration_failed").into_response())
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
            .map_err(|e| PasskeyError::StoreError(e.to_string()))?;

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
                session.insert("add_passkey_reg_state", reg_state).await?;

                // Serialize challenge to JSON
                let challenge_json = serde_json::to_string(&ccr)?;

                // Return challenge page
                let markup = add_passkey_challenge_page(&email, &challenge_json);
                Ok((
                    StatusCode::OK,
                    [(header::CONTENT_TYPE, "text/html")],
                    markup,
                )
                    .into_response())
            }
            Err(_) => Ok(Redirect::to("/me?error=registration_failed").into_response()),
        }
    }
}

fn add_passkey_challenge_page(email: &str, challenge_data: &str) -> maud::Markup {
    let content = html! {
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

        p class="text-center text-sm/6 text-gray-600 dark:text-gray-400" {
            "Email: " strong { (email) }
        }

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
    };

    layout(None, content)
}

id!(PasskeyId, Ident::new16());

#[derive(Debug, Clone)]
pub struct Passkey {
    pub id: PasskeyId,
    pub passkey: webauthn_rs::prelude::Passkey,
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum PasskeyEvent {
    Created {
        user_id: UserId,
        passkey: webauthn_rs::prelude::Passkey,
    },
    Deleted {
        user_id: UserId,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum PasskeyStoreError {
    #[error("Storage operation failed: {0}")]
    #[expect(dead_code)]
    OperationFailed(String),
}

pub trait PasskeyStore: EventStore<Id = PasskeyId, Event = PasskeyEvent> {
    async fn get_user_passkeys(&self, user_id: &UserId) -> Result<Vec<Passkey>, Self::Error>;

    async fn get_all_credentials(&self) -> Result<Vec<webauthn_rs::prelude::Passkey>, Self::Error>;

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

impl EventStore for MemoryPasskeyStore {
    type Id = PasskeyId;
    type Event = PasskeyEvent;
    type EventId = ();
    type Error = PasskeyStoreError;

    async fn record(
        &self,
        id: PasskeyId,
        by: Authority,
        event: PasskeyEvent,
    ) -> Result<(), PasskeyStoreError> {
        let mut data = self.data.lock().await;
        let _ = by; // Store doesn't use authority yet, but will for audit trail

        match event {
            PasskeyEvent::Created { user_id, passkey } => {
                let passkeys = data.keys.entry(user_id).or_insert_with(Vec::new);
                passkeys.push(Passkey { id, passkey });
            }
            PasskeyEvent::Deleted { user_id } => {
                if let Some(passkeys) = data.keys.get_mut(&user_id) {
                    passkeys.retain(|stored| stored.id != id);
                }
            }
        }

        Ok(())
    }
}

impl PasskeyStore for MemoryPasskeyStore {
    async fn get_user_passkeys(&self, user_id: &UserId) -> Result<Vec<Passkey>, PasskeyStoreError> {
        let data = self.data.lock().await;
        Ok(data.keys.get(user_id).cloned().unwrap_or_default())
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

        // Deleting non-existent passkey should succeed silently
        let passkey_id = PasskeyId::new();
        passkey_store
            .record(
                passkey_id,
                Authority::Direct(Actor::User(user_id)),
                PasskeyEvent::Deleted { user_id },
            )
            .await
            .expect("Should succeed even for non-existent passkey");

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
