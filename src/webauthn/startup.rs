use crate::webauthn::error::WebauthnError;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use webauthn_rs::prelude::*;

/*
 * Webauthn RS server side app state and setup code.
 */

// Configure the Webauthn instance by using the WebauthnBuilder. This defines
// the options needed for your site, and has CRITICAL SECURITY IMPLICATIONS:
//
// 1. rp_id (Relying Party ID): You can NOT change this without invalidating ALL
//    existing webauthn credentials. It must match or be a suffix of your domain.
//
// 2. rp_origin (Relying Party Origin): This must exactly match your site's origin
//    (protocol + domain + port). The browser validates this for security.
//
// Both parameters are passed explicitly to prevent accidental misconfiguration.

pub struct Data {
    pub name_to_id: HashMap<String, Uuid>,
    pub keys: HashMap<Uuid, Vec<Passkey>>,
}

#[derive(Clone)]
pub struct AppState {
    // Webauthn has no mutable inner state, so Arc and read only is sufficent.
    // Alternately, you could use a reference here provided you can work out
    // lifetimes.
    pub webauthn: Arc<Webauthn>,
    // This needs mutability, so does require a mutex.
    pub users: Arc<Mutex<Data>>,
}

impl AppState {
    /// Creates a new AppState with explicit WebAuthn security parameters.
    ///
    /// # Security Critical Parameters
    /// - `rp_id`: Relying Party ID - CANNOT be changed without invalidating all credentials
    /// - `rp_origin`: Relying Party Origin - must match the actual site origin exactly
    ///
    /// # Errors
    /// Returns error if the WebAuthn configuration is invalid (mismatched rp_id/origin)
    pub fn new(rp_id: &str, rp_origin: Url) -> Result<Self, WebauthnError> {
        let builder = WebauthnBuilder::new(rp_id, &rp_origin)?.rp_name("Monkesto");
        let webauthn = Arc::new(builder.build()?);
        let users = Arc::new(Mutex::new(Data {
            name_to_id: HashMap::new(),
            keys: HashMap::new(),
        }));
        Ok(AppState { webauthn, users })
    }
}
