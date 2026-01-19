mod authority;
mod error;
mod passkey;
mod signin;
mod signout;
mod signup;
pub mod user;

use axum::{
    Router,
    extract::Extension,
    response::{IntoResponse, Redirect},
    routing::get,
};
use axum_login::login_required;
use std::{env, sync::Arc};
use webauthn_rs::prelude::{Url, WebauthnBuilder};

use error::WebauthnError;
use passkey::MemoryPasskeyStore;
pub use user::MemoryUserStore;

pub type AuthSession = axum_login::AuthSession<MemoryUserStore>;

pub fn router<S: Clone + Send + Sync + 'static>(
    user_store: Arc<MemoryUserStore>,
) -> Result<Router<S>, WebauthnError> {
    // Get base URL from environment variable, defaulting to localhost:3000
    let base_url = env::var("RAILWAY_PUBLIC_DOMAIN")
        .ok()
        .map(|f| format!("https://{}", f))
        .unwrap_or_else(|| {
            env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
        });

    let webauthn_url = format!("{}/webauthn/", base_url);

    // Parse the base URL to extract rp_id and rp_origin for WebAuthn security
    let rp_origin = Url::parse(&base_url)?;
    let rp_id = rp_origin.host_str().ok_or(WebauthnError::InvalidHost)?;

    // Create WebAuthn instance and passkey storage
    let webauthn = Arc::new(
        WebauthnBuilder::new(rp_id, &rp_origin)?
            .rp_name("Monkesto")
            .build()?,
    );
    let passkey_store = Arc::new(MemoryPasskeyStore::new());

    // Protected routes (require login)
    let protected_routes = Router::new()
        .route(
            "/passkey",
            get(passkey::passkey_get::<MemoryUserStore, MemoryPasskeyStore>)
                .post(passkey::create_passkey_post::<MemoryUserStore, MemoryPasskeyStore>),
        )
        .route(
            "/passkey/{id}/delete",
            axum::routing::post(passkey::delete_passkey_post::<MemoryPasskeyStore>),
        )
        .route("/signout", get(signout::signout_get))
        .route("/signout", axum::routing::post(signout::signout_post))
        .route_layer(login_required!(
            MemoryUserStore,
            login_url = "/webauthn/signin"
        ));

    // Public routes (no login required)
    let public_routes = Router::new()
        .route("/", get(redirect_to_signin))
        .route(
            "/signin",
            get(signin::signin_get::<MemoryPasskeyStore>)
                .post(signin::signin_post::<MemoryUserStore, MemoryPasskeyStore>),
        )
        .route(
            "/signup",
            get(signup::signup_get)
                .post(signup::signup_post::<MemoryUserStore, MemoryPasskeyStore>),
        );

    Ok(public_routes
        .merge(protected_routes)
        .layer(Extension(webauthn_url))
        .layer(Extension(webauthn))
        .layer(Extension(user_store))
        .layer(Extension(passkey_store)))
}

async fn redirect_to_signin() -> impl IntoResponse {
    Redirect::temporary("/webauthn/signin")
}
