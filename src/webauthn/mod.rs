mod authority;
mod error;
mod passkey;
mod signin;
mod signout;
mod signup;
mod storage;
pub mod user;

use axum::{
    Router,
    extract::Extension,
    response::{IntoResponse, Redirect},
    routing::get,
};
use std::{env, sync::Arc};
use tower_sessions::{
    Expiry, MemoryStore, SessionManagerLayer,
    cookie::{SameSite, time::Duration},
};
use webauthn_rs::prelude::{Url, WebauthnBuilder};

use error::WebauthnError;
use passkey::MemoryPasskeyStore;
use user::MemoryUserStore;

pub fn router<S: Clone + Send + Sync + 'static>() -> Result<Router<S>, WebauthnError> {
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

    // Create WebAuthn instance and default storage
    let webauthn = Arc::new(
        WebauthnBuilder::new(rp_id, &rp_origin)?
            .rp_name("Monkesto")
            .build()?,
    );
    let user_store = Arc::new(MemoryUserStore::new());
    let passkey_store = Arc::new(MemoryPasskeyStore::new());

    Ok(Router::new()
        .route("/", get(redirect_to_signin))
        .route(
            "/signin",
            get(signin::signin_get::<MemoryPasskeyStore>)
                .post(signin::signin_post::<MemoryPasskeyStore>),
        )
        .route(
            "/signup",
            get(signup::signup_get)
                .post(signup::signup_post::<MemoryUserStore, MemoryPasskeyStore>),
        )
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
        .layer(Extension(webauthn_url))
        .layer(Extension(webauthn))
        .layer(Extension(user_store))
        .layer(Extension(passkey_store))
        .layer(
            SessionManagerLayer::new(MemoryStore::default())
                .with_name("webauthnrs")
                .with_same_site(SameSite::Lax)
                .with_secure(true)
                .with_expiry(Expiry::OnInactivity(Duration::seconds(360))),
        ))
}

async fn redirect_to_signin() -> impl IntoResponse {
    Redirect::temporary("/webauthn/signin")
}
