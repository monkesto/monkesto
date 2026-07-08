use super::PasskeyEvent;
use super::UserId;
use super::layout::layout;
use super::user::User;
pub(crate) use super::{AuthEvent, AuthInterface, AuthSession, PasskeyId};
use crate::authority::Actor;
use crate::authority::Authority;
use crate::time_provider::{DefaultTimeProvider, TimeProvider, TimeStamp};
use axum::extract::Extension;
use axum::extract::Form;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::http::header;
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::response::Response;
use maud::PreEscaped;
use maud::html;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;
use thiserror::Error;
use webauthn_rs::prelude::PasskeyRegistration;
use webauthn_rs::prelude::RegisterPublicKeyCredential;
use webauthn_rs::prelude::Webauthn;

/// Errors that occur during passkey management operations.
#[derive(Error, Debug)]
pub enum PasskeyError {
    #[error("Session expired")]
    SessionExpired,
    #[error("Invalid input data")]
    InvalidInput,
    #[error("Session error: {0}")]
    SessionError(#[from] tower_sessions::session::Error),
    #[error("a passkey with the id {0} already exists")]
    IdConflict(PasskeyId),
    #[error("no passkey exists with the provided id: {0}")]
    PasskeyDoesntExist(PasskeyId),
    #[error("no user exists with the provided id: {0}")]
    UserDoesntExist(UserId),
    #[error("failed to serialize a value with serde_json")]
    Json(#[from] serde_json::Error),
    #[error("received an error from sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct CorePasskey(pub webauthn_rs::prelude::Passkey);

// todo: figure out why this wasn't implemented in the original type
impl Eq for CorePasskey {}

impl Deref for CorePasskey {
    type Target = webauthn_rs::prelude::Passkey;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct PasskeyState {
    pub id: PasskeyId,
    pub user_id: UserId,
    pub passkey: MsgPack<CorePasskey>,
}

#[derive(Debug, StateQuery, Clone, Serialize, Deserialize)]
#[state_query(PasskeyEvent)]
pub struct Passkey {
    #[id]
    passkey_id: PasskeyId,
    user_id: UserId,
    // passkey being Some(_) is the `found` discriminator for this type
    passkey: Option<CorePasskey>,
    deleted: bool,
}

impl Passkey {
    fn new(passkey_id: PasskeyId, user_id: UserId) -> Self {
        Self {
            passkey_id,
            user_id,
            passkey: None,
            deleted: false,
        }
    }
}

impl StateMutate for Passkey {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            PasskeyEvent::PasskeyCreated {
                user_id, passkey, ..
            } => {
                self.user_id = user_id;
                self.passkey = Some(*passkey);
            }
            PasskeyEvent::PasskeyDeleted { .. } => {
                self.deleted = true;
            }
        }
    }
}

pub struct CreatePasskey {
    passkey_id: PasskeyId,
    user_id: UserId,
    passkey: CorePasskey,
    authority: Authority,
    timestamp: TimeStamp,
}

impl CreatePasskey {
    pub(crate) fn new(
        passkey_id: PasskeyId,
        user_id: UserId,
        passkey: CorePasskey,
        authority: Authority,
        timestamp: TimeStamp,
    ) -> Self {
        Self {
            passkey_id,
            user_id,
            passkey,
            authority,
            timestamp,
        }
    }
}

impl Decision for CreatePasskey {
    type Event = AuthEvent;
    type StateQuery = (User, Passkey);
    type Error = PasskeyError;

    fn state_query(&self) -> Self::StateQuery {
        (
            User::new(self.user_id),
            Passkey::new(self.passkey_id, self.user_id),
        )
    }

    fn process(&self, (user, passkey): &Self::StateQuery) -> Result<Vec<Self::Event>, Self::Error> {
        if !user.found || user.deleted {
            return Err(PasskeyError::UserDoesntExist(user.user_id));
        }

        if passkey.passkey.is_some() {
            return Err(PasskeyError::IdConflict(passkey.passkey_id));
        }

        Ok(vec![AuthEvent::PasskeyCreated {
            passkey_id: self.passkey_id,
            user_id: self.user_id,
            passkey: Box::new(self.passkey.clone()),
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
}

pub struct DeletePasskey {
    passkey_id: PasskeyId,
    user_id: UserId,
    authority: Authority,
    timestamp: TimeStamp,
}

impl DeletePasskey {
    fn new(
        passkey_id: PasskeyId,
        user_id: UserId,
        authority: Authority,
        timestamp: TimeStamp,
    ) -> Self {
        Self {
            passkey_id,
            user_id,
            authority,
            timestamp,
        }
    }
}

impl Decision for DeletePasskey {
    type Event = AuthEvent;
    type StateQuery = (User, Passkey);
    type Error = PasskeyError;

    fn state_query(&self) -> Self::StateQuery {
        (
            User::new(self.user_id),
            Passkey::new(self.passkey_id, self.user_id),
        )
    }

    fn process(&self, (user, passkey): &Self::StateQuery) -> Result<Vec<Self::Event>, Self::Error> {
        if !user.found || user.deleted {
            return Err(PasskeyError::UserDoesntExist(user.user_id));
        }

        if passkey.passkey.is_none() || passkey.deleted {
            return Err(PasskeyError::PasskeyDoesntExist(passkey.passkey_id));
        }

        Ok(vec![AuthEvent::PasskeyDeleted {
            passkey_id: self.passkey_id,
            authority: self.authority.clone(),
            timestamp: self.timestamp,
        }])
    }
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
            PasskeyError::IdConflict(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "Id conflict").into_response()
            }
            PasskeyError::PasskeyDoesntExist(_) => {
                (StatusCode::BAD_REQUEST, "Passkey doesnt exist").into_response()
            }
            PasskeyError::UserDoesntExist(_) => {
                (StatusCode::BAD_REQUEST, "User doesnt exist").into_response()
            }
            PasskeyError::Json(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to encode/parse json",
            )
                .into_response(),
            PasskeyError::Sqlx(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Failed to interact with the database",
            )
                .into_response(),
        }
    }
}

use crate::postcard::MsgPack;
use disintegrate::{Decision, StateMutate, StateQuery};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

pub async fn delete_passkey_post(
    Extension(interface): Extension<AuthInterface>,
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
    if interface
        .decision_maker
        .make(DeletePasskey::new(
            passkey_id,
            user_id,
            Authority::Direct(Actor::User(user_id)),
            DefaultTimeProvider.get_time(),
        ))
        .await
        .is_err()
    {
        return Ok(Redirect::to("/me?error=passkeydeletionfailure").into_response());
    }

    // Redirect back to passkey page
    Ok(Redirect::to("/me").into_response())
}

pub async fn create_passkey_post(
    Extension(webauthn): Extension<Arc<Webauthn>>,
    Extension(auth_interface): Extension<AuthInterface>,
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
                _ = session.remove_value("add_passkey_reg_state").await;

                // Generate a PasskeyId for this passkey
                let passkey_id = PasskeyId::new();

                // Add the new passkey to the user's existing passkeys
                if auth_interface
                    .decision_maker
                    .make(CreatePasskey::new(
                        passkey_id,
                        user_id,
                        CorePasskey(passkey),
                        Authority::Direct(Actor::User(user_id)),
                        DefaultTimeProvider.get_time(),
                    ))
                    .await
                    .is_err()
                {
                    return Ok(Redirect::to("/signup?error=passkeycreationfailure").into_response());
                }

                // Redirect back to passkey management page
                Ok(Redirect::to("/me").into_response())
            }
            Err(_) => {
                // Clear the registration state on failure
                _ = session.remove_value("add_passkey_reg_state").await;
                Ok(Redirect::to("/me?error=registration_failed").into_response())
            }
        }
    } else {
        // This is initial request - start registration
        // Get user's existing passkeys
        let existing_passkeys = auth_interface
            .get_user_passkeys(user_id)
            .await
            .unwrap_or_default();

        let user = auth_interface
            .query_user(user_id)
            .await
            .map_err(|_| PasskeyError::UserDoesntExist(user_id))?;

        let exclude_credentials: Vec<_> = existing_passkeys
            .iter()
            .map(|stored| stored.passkey.cred_id().clone())
            .collect();

        // Clear any previous registration state
        _ = session.remove_value("add_passkey_reg_state").await;

        // Start passkey registration
        match webauthn.start_passkey_registration(
            user.webauthn_uuid,
            user.email.as_ref(),
            user.email.as_ref(),
            Some(exclude_credentials),
        ) {
            Ok((ccr, reg_state)) => {
                // Store registration state in session
                session.insert("add_passkey_reg_state", reg_state).await?;

                // Serialize challenge to JSON
                let challenge_json = serde_json::to_string(&ccr)?;

                // Return challenge page
                let markup = add_passkey_challenge_page(user.email.as_ref(), &challenge_json);
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
        div class="flex flex-col gap-6 sm:mx-auto sm:w-full sm:max-w-sm" {
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
        }
    };

    layout(None, content)
}
