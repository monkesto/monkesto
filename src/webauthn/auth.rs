use super::error::WebauthnError;
use super::startup::AppState;
use axum::{
    extract::{Extension, Json, Path},
    http::StatusCode,
    response::IntoResponse,
};
use base64::Engine;
use tower_sessions::Session;

/*
 * Webauthn RS auth handlers.
 * These files use webauthn to process the data received from each route, and are closely tied to axum
 */

// 1. Import the prelude - this contains everything needed for the server to function.
use webauthn_rs::prelude::*;

// 2. The first step a client (user) will carry out is requesting a credential to be
// registered. We need to provide a challenge for this. The work flow will be:
//
//          ┌───────────────┐     ┌───────────────┐      ┌───────────────┐
//          │ Authenticator │     │    Browser    │      │     Site      │
//          └───────────────┘     └───────────────┘      └───────────────┘
//                  │                     │                      │
//                  │                     │     1. Start Reg     │
//                  │                     │─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─▶│
//                  │                     │                      │
//                  │                     │     2. Challenge     │
//                  │                     │◀ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┤
//                  │                     │                      │
//                  │  3. Select Token    │                      │
//             ─ ─ ─│◀ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│                      │
//  4. Verify │     │                     │                      │
//                  │  4. Yield PubKey    │                      │
//            └ ─ ─▶│─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─▶                      │
//                  │                     │                      │
//                  │                     │  5. Send Reg Opts    │
//                  │                     │─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─▶│─ ─ ─
//                  │                     │                      │     │ 5. Verify
//                  │                     │                      │         PubKey
//                  │                     │                      │◀─ ─ ┘
//                  │                     │                      │─ ─ ─
//                  │                     │                      │     │ 6. Persist
//                  │                     │                      │       Credential
//                  │                     │                      │◀─ ─ ┘
//                  │                     │                      │
//                  │                     │                      │
//
// In this step, we are responding to the start reg(istration) request, and providing
// the challenge to the browser.

pub async fn start_register(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, WebauthnError> {
    info!("Start register");
    // We get the username from the URL, but you could get this via form submission or
    // some other process. In some parts of Webauthn, you could also use this as a "display name"
    // instead of a username. Generally you should consider that the user *can* and *will* change
    // their username at any time.

    // Since a user's username could change at anytime, we need to bind to a unique id.
    // We use uuid's for this purpose, and you should generate these randomly. If the
    // username does exist and is found, we can match back to our unique id. This is
    // important in authentication, where presented credentials may *only* provide
    // the unique id, and not the username!

    let user_unique_id = {
        let users_guard = app_state.users.lock().await;
        users_guard
            .name_to_id
            .get(&username)
            .copied()
            .unwrap_or_else(Uuid::new_v4)
    };

    // Remove any previous registrations that may have occured from the session.
    let _ = session.remove_value("reg_state").await;

    // If the user has any other credentials, we exclude these here so they can't be duplicate registered.
    // It also hints to the browser that only new credentials should be "blinked" for interaction.
    let exclude_credentials = {
        let users_guard = app_state.users.lock().await;
        users_guard
            .keys
            .get(&user_unique_id)
            .map(|keys| keys.iter().map(|sk| sk.cred_id().clone()).collect())
    };

    let res = match app_state.webauthn.start_passkey_registration(
        user_unique_id,
        &username,
        &username,
        exclude_credentials,
    ) {
        Ok((ccr, reg_state)) => {
            // Note that due to the session store in use being a server side memory store, this is
            // safe to store the reg_state into the session since it is not client controlled and
            // not open to replay attacks. If this was a cookie store, this would be UNSAFE.
            session
                .insert("reg_state", (username, user_unique_id, reg_state))
                .await
                .expect("Failed to insert");
            info!("Registration Successful!");
            Json(ccr)
        }
        Err(e) => {
            info!("challenge_register -> {:?}", e);
            return Err(WebauthnError::Unknown);
        }
    };
    Ok(res)
}

// 3. The browser has completed it's steps and the user has created a public key
// on their device. Now we have the registration options sent to us, and we need
// to verify these and persist them.

pub async fn finish_register(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Json(reg): Json<RegisterPublicKeyCredential>,
) -> Result<impl IntoResponse, WebauthnError> {
    let (username, user_unique_id, reg_state) = match session.get("reg_state").await? {
        Some((username, user_unique_id, reg_state)) => (username, user_unique_id, reg_state),
        None => {
            error!("Failed to get session");
            return Err(WebauthnError::CorruptSession);
        }
    };

    let _ = session.remove_value("reg_state").await;

    let res = match app_state
        .webauthn
        .finish_passkey_registration(&reg, &reg_state)
    {
        Ok(sk) => {
            let mut users_guard = app_state.users.lock().await;

            //TODO: This is where we would store the credential in a db, or persist them in some other way.
            users_guard
                .keys
                .entry(user_unique_id)
                .and_modify(|keys| {
                    info!(
                        "Adding credential to existing user {:?}, now has {} credentials",
                        user_unique_id,
                        keys.len() + 1
                    );
                    keys.push(sk.clone());
                })
                .or_insert_with(|| {
                    info!(
                        "Creating new user {:?} with first credential",
                        user_unique_id
                    );
                    vec![sk.clone()]
                });

            users_guard.name_to_id.insert(username, user_unique_id);

            info!(
                "Registration complete - total users: {}, total credentials: {}",
                users_guard.name_to_id.len(),
                users_guard.keys.values().map(|v| v.len()).sum::<usize>()
            );

            StatusCode::OK
        }
        Err(e) => {
            error!("challenge_register -> {:?}", e);
            StatusCode::BAD_REQUEST
        }
    };

    Ok(res)
}

// 4. Now that our public key has been registered, we can authenticate a user and verify
// that they are the holder of that security token. The work flow is similar to registration.
//
//          ┌───────────────┐     ┌───────────────┐      ┌───────────────┐
//          │ Authenticator │     │    Browser    │      │     Site      │
//          └───────────────┘     └───────────────┘      └───────────────┘
//                  │                     │                      │
//                  │                     │     1. Start Auth    │
//                  │                     │─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─▶│
//                  │                     │                      │
//                  │                     │     2. Challenge     │
//                  │                     │◀ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ┤
//                  │                     │                      │
//                  │  3. Select Token    │                      │
//             ─ ─ ─│◀ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─│                      │
//  4. Verify │     │                     │                      │
//                  │    4. Yield Sig     │                      │
//            └ ─ ─▶│─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─▶                      │
//                  │                     │    5. Send Auth      │
//                  │                     │        Opts          │
//                  │                     │─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─▶│─ ─ ─
//                  │                     │                      │     │ 5. Verify
//                  │                     │                      │          Sig
//                  │                     │                      │◀─ ─ ┘
//                  │                     │                      │
//                  │                     │                      │
//
// The user indicates the wish to start authentication and we need to provide a challenge.

pub async fn start_authentication(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, WebauthnError> {
    info!("Start Authentication");
    // We get the username from the URL, but you could get this via form submission or
    // some other process.

    // Remove any previous authentication that may have occured from the session.
    let _ = session.remove_value("auth_state").await;

    // Get the set of keys that the user possesses
    let users_guard = app_state.users.lock().await;

    // Look up their unique id from the username
    let user_unique_id = users_guard
        .name_to_id
        .get(&username)
        .copied()
        .ok_or(WebauthnError::UserNotFound)?;

    let allow_credentials = users_guard
        .keys
        .get(&user_unique_id)
        .ok_or(WebauthnError::UserHasNoCredentials)?;

    let res = match app_state
        .webauthn
        .start_passkey_authentication(allow_credentials)
    {
        Ok((rcr, auth_state)) => {
            // Drop the mutex to allow the mut borrows below to proceed
            drop(users_guard);

            // Note that due to the session store in use being a server side memory store, this is
            // safe to store the auth_state into the session since it is not client controlled and
            // not open to replay attacks. If this was a cookie store, this would be UNSAFE.
            session
                .insert("auth_state", (user_unique_id, auth_state))
                .await
                .expect("Failed to insert");
            Json(rcr)
        }
        Err(e) => {
            info!("challenge_authenticate -> {:?}", e);
            return Err(WebauthnError::Unknown);
        }
    };
    Ok(res)
}

// 5. The browser and user have completed their part of the processing. Only in the
// case that the webauthn authenticate call returns Ok, is authentication considered
// a success. If the browser does not complete this call, or *any* error occurs,
// this is an authentication failure.

pub async fn finish_authentication(
    Extension(app_state): Extension<AppState>,
    session: Session,
    Json(auth): Json<PublicKeyCredential>,
) -> Result<impl IntoResponse, WebauthnError> {
    // Debug: log the credential ID from the authentication response
    info!("Authentication response credential ID: {:?}", auth.id);
    // First try to get regular auth state, then try usernameless auth state
    let (user_unique_id, auth_state): (Uuid, PasskeyAuthentication) =
        if let Some((user_unique_id, auth_state)) = session
            .get::<(Uuid, PasskeyAuthentication)>("auth_state")
            .await?
        {
            let _ = session.remove_value("auth_state").await;
            (user_unique_id, auth_state)
        } else if let Some(auth_state) = session
            .get::<PasskeyAuthentication>("usernameless_auth_state")
            .await?
        {
            let _ = session.remove_value("usernameless_auth_state").await;
            // For usernameless auth, we need to determine the user_unique_id from the credential
            // We'll do this after the webauthn verification
            (Uuid::nil(), auth_state)
        } else {
            return Err(WebauthnError::CorruptSession);
        };

    // For usernameless auth, use the auth_state that contains all credentials
    if user_unique_id == Uuid::nil() {
        info!("Usernameless auth: verifying credential ID: {}", auth.id);

        match app_state
            .webauthn
            .finish_passkey_authentication(&auth, &auth_state)
        {
            Ok(auth_result) => {
                info!("Usernameless authentication successful, identifying user...");

                // Find which user owns this credential after successful verification
                let users_guard = app_state.users.lock().await;
                let found_user_id = users_guard.keys.iter().find_map(|(uid, keys)| {
                    keys.iter()
                        .any(|sk| {
                            let stored_cred_id_b64 =
                                base64::engine::general_purpose::URL_SAFE_NO_PAD
                                    .encode(sk.cred_id().as_ref());
                            stored_cred_id_b64 == auth.id
                        })
                        .then_some(*uid)
                });

                if let Some(user_id) = found_user_id {
                    info!("Identified user: {:?}", user_id);
                    drop(users_guard);
                    let mut users_guard = app_state.users.lock().await;

                    // Update the credential counter
                    users_guard
                        .keys
                        .get_mut(&user_id)
                        .map(|keys| {
                            keys.iter_mut().for_each(|sk| {
                                sk.update_credential(&auth_result);
                            })
                        })
                        .ok_or(WebauthnError::UserHasNoCredentials)?;

                    info!("Authentication Successful!");
                    return Ok(StatusCode::OK);
                } else {
                    error!("Could not identify user for verified credential");
                    return Err(WebauthnError::UserNotFound);
                }
            }
            Err(e) => {
                error!("usernameless authentication verification failed: {:?}", e);
                return Ok(StatusCode::BAD_REQUEST);
            }
        }
    }

    // Regular authentication with known user
    match app_state
        .webauthn
        .finish_passkey_authentication(&auth, &auth_state)
    {
        Ok(auth_result) => {
            let mut users_guard = app_state.users.lock().await;

            users_guard
                .keys
                .get_mut(&user_unique_id)
                .map(|keys| {
                    keys.iter_mut().for_each(|sk| {
                        sk.update_credential(&auth_result);
                    })
                })
                .ok_or(WebauthnError::UserHasNoCredentials)?;
            info!("Authentication Successful!");
            Ok(StatusCode::OK)
        }
        Err(e) => {
            error!("challenge_authenticate -> {:?}", e);
            Ok(StatusCode::BAD_REQUEST)
        }
    }
}

// Usernameless authentication start - allows login without specifying a username
// Uses discoverable/resident keys where the authenticator presents available credentials
pub async fn start_usernameless_authentication(
    Extension(app_state): Extension<AppState>,
    session: Session,
) -> Result<impl IntoResponse, WebauthnError> {
    info!("Start Usernameless Authentication");

    // Remove any previous authentication that may have occured from the session.
    let _ = session.remove_value("auth_state").await;
    info!("Removed previous auth_state from session");

    // Due to webauthn-rs limitations, we need credentials for verification
    // Load all credentials but ensure client receives empty allowCredentials for privacy
    let users_guard = app_state.users.lock().await;
    info!(
        "Loading credentials for usernameless auth - users: {}, credential entries: {}",
        users_guard.name_to_id.len(),
        users_guard.keys.len()
    );

    for (uid, keys) in &users_guard.keys {
        info!("User {:?} has {} credentials", uid, keys.len());
    }

    let all_credentials: Vec<_> = users_guard.keys.values().flatten().cloned().collect();
    drop(users_guard);

    if all_credentials.is_empty() {
        let users_guard = app_state.users.lock().await;
        info!(
            "No credentials found - total users: {}, credential entries: {}",
            users_guard.name_to_id.len(),
            users_guard.keys.len()
        );
        drop(users_guard);
        return Err(WebauthnError::UserHasNoCredentials);
    }

    info!(
        "Creating usernameless auth challenge with {} credentials for verification",
        all_credentials.len()
    );
    let res = match app_state
        .webauthn
        .start_passkey_authentication(&all_credentials)
    {
        Ok((mut rcr, auth_state)) => {
            info!("Successfully created usernameless authentication challenge");

            // Clear allowCredentials to ensure true usernameless experience
            // Client won't see any credential hints, maintaining privacy
            rcr.public_key.allow_credentials.clear();

            // Store the auth state - we'll identify the user from their credential response
            match session.insert("usernameless_auth_state", auth_state).await {
                Ok(_) => {
                    info!("Successfully stored auth state in session");
                    Json(rcr)
                }
                Err(e) => {
                    error!("Failed to insert auth state into session: {:?}", e);
                    return Err(WebauthnError::Unknown);
                }
            }
        }
        Err(e) => {
            error!("start_usernameless_authentication -> {:?}", e);
            return Err(WebauthnError::Unknown);
        }
    };
    Ok(res)
}
