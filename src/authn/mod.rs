mod corepasskey;
mod layout;
mod me;
pub mod passkey;
mod signin;
mod signout;
mod signup;
mod store;
pub mod user;

use crate::id::Ident;

use crate::authn::corepasskey::CorePasskey;
use crate::authn::passkey::{CreatePasskey, DeletePasskey, PasskeyError, PasskeyState};
use crate::authn::user::{CreateUser, DEV_USERS, UserError, UserResult, UserState};
use crate::authority::Authority;
use crate::email::Email;
use crate::event_id::GetEventId;
use crate::monkesto_error::OrRedirect;
use crate::time_provider::Timestamp;
use crate::{id, shutdown};
use async_trait::async_trait;
use axum::Router;
use axum::extract::Extension;
use axum::response::Redirect;
use axum::routing::get;
use axum::routing::post;
use axum_login::{AuthnBackend, login_required, tracing};
use disintegrate::serde::messagepack::MessagePack;
use disintegrate::{DecisionError, Event, EventListener, PersistedEvent, StreamQuery, query};
use disintegrate_postgres::{
    PgDecisionMaker, PgEventId, PgEventListener, PgEventListenerConfig, PgEventListenerError,
    PgSnapshotter, RetryAction, WithPgSnapshot, decision_maker,
};
pub use layout::layout;
use serde::{Deserialize, Serialize};
use sqlx::postgres::PgHasArrayType;
use sqlx::{Database, PgPool, Postgres, Type};
use std::env;
use std::sync::Arc;
use std::time::Duration;
pub use store::AuthnEventStore;
use thiserror::Error;
use tokio::sync::watch;
use webauthn_rs::prelude::WebauthnBuilder;
use webauthn_rs::prelude::WebauthnError as WebauthnCoreError;
use webauthn_rs::prelude::{CredentialID, Url, Uuid};

pub type AuthSession = axum_login::AuthSession<AuthnService>;

/// Errors that occur during WebAuthn router initialization/configuration.
/// These are startup-time errors, not request-handling errors.
#[derive(Error, Debug)]
pub enum AuthConfigError {
    #[error("WebAuthn initialization failed: {0}")]
    WebauthnInit(#[from] WebauthnCoreError),
    #[error("Invalid URL for WebAuthn origin: {0}")]
    InvalidUrl(#[from] url::ParseError),
    #[error("BASE_URL must have a valid host for WebAuthn rp_id")]
    InvalidHost,
}

#[derive(Debug, Error)]
pub enum AuthConnectError {
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    #[error("disintegrate error: {0}")]
    Disintegrate(String),
}

id!(UserId, Ident::new16());
id!(PasskeyId, Ident::new16());

type PgAuthnDecisionMaker = PgDecisionMaker<AuthnEvent, MessagePack<AuthnEvent>, WithPgSnapshot>;

#[derive(Debug, Clone, PartialEq, Event, Serialize, Deserialize)]
#[stream(UserEvent, [UserCreated, UserDeleted])]
#[stream(PasskeyEvent, [PasskeyCreated, PasskeyDeleted])]
pub enum AuthnEvent {
    UserCreated {
        #[id]
        user_id: UserId,
        #[id]
        email: Email,
        webauthn_uuid: Uuid,
        authority: Authority,
        timestamp: Timestamp,
    },
    UserDeleted {
        #[id]
        user_id: UserId,
        authority: Authority,
        timestamp: Timestamp,
    },
    PasskeyCreated {
        #[id]
        passkey_id: PasskeyId,
        user_id: UserId,
        passkey: Box<CorePasskey>,
        authority: Authority,
        timestamp: Timestamp,
    },
    PasskeyDeleted {
        #[id]
        passkey_id: PasskeyId,
        authority: Authority,
        timestamp: Timestamp,
    },
}

#[derive(Clone)]
pub struct AuthnService {
    query: StreamQuery<PgEventId, AuthnEvent>,
    projection_pool: PgPool,
    decision_maker: PgAuthnDecisionMaker,
    current_event: watch::Sender<PgEventId>,
}

impl PgHasArrayType for UserId {
    fn array_type_info() -> <Postgres as Database>::TypeInfo {
        <&[&str] as Type<Postgres>>::type_info()
    }
}

impl AuthnService {
    pub async fn try_new(
        pool: PgPool,
        event_store: &AuthnEventStore,
    ) -> Result<Self, AuthConnectError> {
        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                email TEXT NOT NULL,
                webauthn_uuid UUID NOT NULL
            )
        "#
        )
        .execute(&pool)
        .await?;

        sqlx::query!(
            r#"
            CREATE TABLE IF NOT EXISTS passkeys (
                id TEXT PRIMARY KEY,
                user_id BYTEA NOT NULL,
                passkey BYTEA NOT NULL,
                credential_id BYTEA NOT NULL
            )
        "#
        )
        .execute(&pool)
        .await?;

        let snapshotter = PgSnapshotter::try_new(pool.clone(), 10)
            .await
            .map_err(|error| AuthConnectError::Disintegrate(error.to_string()))?;
        let decision_maker = decision_maker(
            event_store.event_store.clone(),
            WithPgSnapshot::new(snapshotter),
        );

        let (sender, receiver) = watch::channel(0);

        Box::leak(Box::new(receiver));

        Ok(Self {
            query: query!(AuthnEvent),
            projection_pool: pool,
            decision_maker,
            current_event: sender,
        })
    }

    pub async fn create_user(
        &self,
        user_id: UserId,
        email: Email,
        webauthn_uuid: Uuid,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<UserError>> {
        Ok(self
            .decision_maker
            .make(CreateUser::new(
                user_id,
                email,
                webauthn_uuid,
                authority,
                timestamp,
            ))
            .await?
            .event_id())
    }

    pub async fn create_passkey(
        &self,
        passkey_id: PasskeyId,
        user_id: UserId,
        passkey: CorePasskey,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<PasskeyError>> {
        Ok(self
            .decision_maker
            .make(CreatePasskey::new(
                passkey_id, user_id, passkey, authority, timestamp,
            ))
            .await?
            .event_id())
    }

    pub async fn delete_passkey(
        &self,
        passkey_id: PasskeyId,
        user_id: UserId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<PasskeyError>> {
        Ok(self
            .decision_maker
            .make(DeletePasskey::new(
                passkey_id, user_id, authority, timestamp,
            ))
            .await?
            .event_id())
    }

    pub async fn email_exists(&self, email: &Email) -> UserResult<bool> {
        Ok(sqlx::query_scalar!(
            r#"
            SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)
        "#,
            email.as_ref()
        )
        .fetch_one(&self.projection_pool)
        .await?
        .unwrap_or(false))
    }

    pub async fn fetch_user(&self, user_id: UserId) -> UserResult<UserState> {
        let user = sqlx::query_as!(
            UserState,
            r#"
            SELECT id as "id: UserId", email as "email: Email", webauthn_uuid FROM users WHERE id = $1
        "#,
            user_id as UserId
        )
        .fetch_optional(&self.projection_pool)
        .await?;

        if let Some(user) = user {
            Ok(user)
        } else {
            Err(UserError::UserDoesntExist(user_id))
        }
    }

    pub async fn fetch_users(&self, ids: &[UserId]) -> UserResult<Vec<UserState>> {
        let users = sqlx::query_as!(
            UserState,
            r#"
            SELECT id as "id: UserId", email as "email: Email", webauthn_uuid FROM users WHERE id = ANY($1)
        "#,
            ids as &[UserId]
        )
            .fetch_all(&self.projection_pool)
            .await?;

        Ok(users)
    }

    pub async fn lookup_user_id(&self, email: &Email) -> UserResult<UserId> {
        let id = sqlx::query_scalar!(
            r#"
            SELECT id as "id: UserId" FROM users WHERE email = $1
        "#,
            email as &Email
        )
        .fetch_optional(&self.projection_pool)
        .await?;

        if let Some(id) = id {
            Ok(id)
        } else {
            Err(UserError::EmailDoesntExist(email.clone()))
        }
    }

    /// Returns dev users for displaying in the dev login form.
    pub async fn get_dev_users(&self) -> Vec<UserState> {
        let mut users = Vec::new();
        for (email, (_, _)) in DEV_USERS.clone() {
            if let Ok(user_id) = self.lookup_user_id(&email).await
                && let Ok(user) = self.fetch_user(user_id).await
            {
                users.push(user);
            }
        }
        users
    }

    pub async fn get_user_passkeys(
        &self,
        user_id: UserId,
    ) -> Result<Vec<PasskeyState>, PasskeyError> {
        let passkeys = sqlx::query_as!(
            PasskeyState,
            r#"
            SELECT id as "id: PasskeyId", user_id as "user_id: UserId", passkey as "passkey: CorePasskey" FROM passkeys WHERE user_id = $1
        "#,
        user_id as UserId)
            .fetch_all(&self.projection_pool)
            .await?;

        Ok(passkeys)
    }

    pub async fn get_all_credentials(&self) -> Result<Vec<CorePasskey>, PasskeyError> {
        let passkeys = sqlx::query_scalar!(
            r#"
            SELECT passkey as "passkey: CorePasskey" FROM passkeys
        "#
        )
        .fetch_all(&self.projection_pool)
        .await?;

        Ok(passkeys)
    }

    pub async fn find_user_by_credential(
        &self,
        credential_id: &CredentialID,
    ) -> Result<Option<(UserId, PasskeyId)>, PasskeyError> {
        Ok(sqlx::query_as!(
            PasskeyState,
            r#"
            SELECT user_id as "user_id: UserId", id as "id: PasskeyId", passkey as "passkey: CorePasskey" FROM passkeys WHERE credential_id = $1
        "#,
        credential_id.as_ref())
            .fetch_optional(&self.projection_pool)
            .await?
            .map(|pk| (pk.user_id, pk.id)))
    }

    pub async fn wait_for(&self, event_id: PgEventId) {
        self.current_event
            .subscribe()
            .wait_for(|curr_id| *curr_id >= event_id)
            .await
            .expect("auth service eventid sender closed");
    }
}

impl AuthnBackend for AuthnService {
    type User = UserState;
    type Credentials = ();
    type Error = UserError;

    async fn authenticate(&self, _creds: Self::Credentials) -> UserResult<Option<UserState>> {
        // Auth is handled separately by webauthn
        // We call session.login() directly after webauthn verifies the user
        Ok(None)
    }

    async fn get_user(&self, user_id: &axum_login::UserId<Self>) -> UserResult<Option<UserState>> {
        Ok(Some(self.fetch_user(*user_id).await?))
    }
}

#[async_trait]
impl EventListener<PgEventId, AuthnEvent> for AuthnService {
    type Error = sqlx::Error;

    fn id(&self) -> &'static str {
        "users/passkeys"
    }

    fn query(&self) -> &StreamQuery<PgEventId, AuthnEvent> {
        &self.query
    }

    async fn handle(
        &self,
        event: PersistedEvent<PgEventId, AuthnEvent>,
    ) -> Result<(), Self::Error> {
        let event_id = event.id();
        match event.into_inner() {
            AuthnEvent::UserCreated {
                user_id,
                email,
                webauthn_uuid,
                ..
            } => {
                sqlx::query!(
                    r#"
                    INSERT INTO users (id, email, webauthn_uuid) VALUES($1, $2, $3) ON CONFLICT DO NOTHING
                "#,
                    user_id as UserId,
                    email as Email,
                    webauthn_uuid
                )
                .execute(&self.projection_pool)
                .await?;
            }
            AuthnEvent::UserDeleted { user_id, .. } => {
                sqlx::query!(
                    r#"
                    DELETE FROM users where id = $1
                "#,
                    user_id as UserId
                )
                .execute(&self.projection_pool)
                .await?;
            }
            AuthnEvent::PasskeyCreated {
                passkey_id,
                user_id,
                passkey,
                ..
            } => {
                sqlx::query!(r#"
                    INSERT INTO passkeys (id, user_id, passkey, credential_id) VALUES($1, $2, $3, $4) ON CONFLICT DO NOTHING
                "#,
                passkey_id as PasskeyId,
                user_id as UserId,
                passkey.as_ref() as &CorePasskey,
                passkey.cred_id().as_ref())
                    .execute(&self.projection_pool)
                    .await?;
            }
            AuthnEvent::PasskeyDeleted { passkey_id, .. } => {
                sqlx::query!(
                    r#"
                    DELETE FROM passkeys where id = $1
                "#,
                    passkey_id as PasskeyId
                )
                .execute(&self.projection_pool)
                .await?;
            }
        }

        self.current_event
            .send(event_id)
            .expect("auth eventid sender closed");

        Ok(())
    }
}

pub(crate) async fn event_listener(event_store: AuthnEventStore, service: AuthnService) {
    PgEventListener::builder(event_store.event_store)
        .register_listener(
            service,
            PgEventListenerConfig::poller(Duration::from_secs(60))
                .with_notifier()
                .fetch_size(100)
                .with_retry(handle_event_listener_retry),
        )
        .start_with_shutdown(shutdown())
        .await
        .expect("event listener failed");
}

pub fn get_user(session: AuthSession) -> Result<UserState, Redirect> {
    session
        .user
        .ok_or(UserError::SessionNotFound)
        .or_redirect("/signin")
}

fn handle_event_listener_retry(
    error: PgEventListenerError<sqlx::Error>,
    _attempts: usize,
) -> RetryAction {
    tracing::error!(?error, "read model listener failed");
    RetryAction::Abort
}

pub fn router<S: Clone + Send + Sync + 'static>(
    authn_service: AuthnService,
) -> Result<Router<S>, AuthConfigError> {
    // Get base URL from environment variable, defaulting to localhost:3000
    let base_url = env::var("RAILWAY_PUBLIC_DOMAIN")
        .ok()
        .map(|f| format!("https://{}", f))
        .unwrap_or_else(|| {
            env::var("BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
        });

    let webauthn_url = format!("{}/", base_url);

    // Parse the base URL to extract rp_id and rp_origin for WebAuthn security
    let rp_origin = Url::parse(&base_url)?;
    let rp_id = rp_origin.host_str().ok_or(AuthConfigError::InvalidHost)?;

    // Create WebAuthn instance and passkey storage
    let webauthn = Arc::new(
        WebauthnBuilder::new(rp_id, &rp_origin)?
            .rp_name("Monkesto")
            .build()?,
    );

    // Protected routes (require login)
    let protected_routes = Router::new()
        .route("/me", get(me::me_get))
        .route("/passkey", post(passkey::create_passkey_post))
        .route("/passkey/{id}/delete", post(passkey::delete_passkey_post))
        .route("/signout", get(signout::signout_get))
        .route("/signout", post(signout::signout_post))
        .route_layer(login_required!(AuthnService, login_url = "/signin"));

    // Public routes (no login required)
    let public_routes = Router::new()
        .route("/signin", get(signin::signin_get).post(signin::signin_post))
        .route("/signup", get(signup::signup_get).post(signup::signup_post));

    Ok(public_routes
        .merge(protected_routes)
        .layer(Extension(webauthn_url))
        .layer(Extension(webauthn))
        .layer(Extension(authn_service)))
}
