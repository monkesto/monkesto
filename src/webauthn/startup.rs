use crate::webauthn::error::WebauthnError;
use crate::webauthn::storage::WebauthnStorage;
use std::sync::Arc;
use webauthn_rs::prelude::{Url, Webauthn, WebauthnBuilder};

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

#[derive(Clone)]
pub struct WebauthnState {
    // Webauthn has no mutable inner state, so Arc and read only is sufficent.
    // Alternately, you could use a reference here provided you can work out
    // lifetimes.
    pub webauthn: Arc<Webauthn>,
    // Storage abstraction for users and passkeys
    pub storage: Arc<dyn WebauthnStorage>,
}

impl WebauthnState {
    /// Creates a new AppState with the provided WebAuthn instance and storage implementation
    pub fn new(webauthn: Arc<Webauthn>, storage: Arc<dyn WebauthnStorage>) -> Self {
        Self { webauthn, storage }
    }

    /// Helper function to build a WebAuthn instance with standard configuration
    ///
    /// # Security Critical Parameters
    /// - `rp_id`: Relying Party ID - CANNOT be changed without invalidating all credentials
    /// - `rp_origin`: Relying Party Origin - must match the actual site origin exactly
    ///
    /// # Errors
    /// Returns error if the WebAuthn configuration is invalid (mismatched rp_id/origin)
    pub fn build_webauthn(rp_id: &str, rp_origin: &Url) -> Result<Arc<Webauthn>, WebauthnError> {
        let builder = WebauthnBuilder::new(rp_id, rp_origin)?.rp_name("Monkesto");
        Ok(Arc::new(builder.build()?))
    }
}
