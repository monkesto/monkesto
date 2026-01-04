use std::collections::HashMap;
use std::env;
use std::sync::Arc;
use tokio::sync::Mutex;
use webauthn_rs::prelude::*;

/*
 * Webauthn RS server side app state and setup  code.
 */

// Configure the Webauthn instance by using the WebauthnBuilder. This defines
// the options needed for your site, and has some implications. One of these is that
// you can NOT change your rp_id (relying party id), without invalidating all
// webauthn credentials. Remember, rp_id is derived from your URL origin, meaning
// that it is your effective domain name.

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
    pub fn new() -> Self {
        // Get base URL from environment variable, defaulting to localhost:3000
        let base_url = env::var("RAILWAY_PUBLIC_DOMAIN")
            .ok()
            .map(|f| format!("https://{}", f))
            .unwrap_or_else(|| {
                env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
            });

        // Parse the base URL to extract the host for rp_id
        let rp_origin = Url::parse(&base_url).expect("Invalid BASE_URL");
        let rp_id = rp_origin
            .host_str()
            .expect("BASE_URL must have a valid host");

        let builder = WebauthnBuilder::new(rp_id, &rp_origin).expect("Invalid configuration");

        // Now, with the builder you can define other options.
        // Set a "nice" relying party name. Has no security properties and
        // may be changed in the future.
        let builder = builder.rp_name("Monkesto");

        // Consume the builder and create our webauthn instance.
        let webauthn = Arc::new(builder.build().expect("Invalid configuration"));

        let users = Arc::new(Mutex::new(Data {
            name_to_id: HashMap::new(),
            keys: HashMap::new(),
        }));

        AppState { webauthn, users }
    }
}
