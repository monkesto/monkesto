pub mod user;
pub mod view;

use std::sync::Arc;

use crate::AppState;
use crate::AuthSession;
use crate::ident::UserId;
use crate::known_errors::MonkestoResult;
use crate::known_errors::{KnownErrors, RedirectOnError};
use crate::webauthn::user::Email;
use async_trait::async_trait;
use axum::extract::{Form, State};
use axum::response::Redirect;
use axum_login::AuthnBackend;
use bcrypt::DEFAULT_COST;
use dashmap::DashMap;
use rand::TryRngCore;
use rand::rngs::OsRng;
use serde::Deserialize;
use tokio::task;
use tokio::task::spawn_blocking;
use user::{UserEvent, UserState};

#[async_trait]
#[allow(dead_code)]
pub trait UserStore: Clone + Send + Sync + AuthnBackend {
    /// creates a new user state in the event store with the data from the creation event
    ///
    /// it should return an error if the event passed in is not a creation event
    async fn create_user(&self, creation_event: UserEvent) -> MonkestoResult<()>;

    /// adds a UserEvent to the event store and updates the cached state
    async fn push_event(&self, user_id: &UserId, event: UserEvent) -> MonkestoResult<()>;

    async fn get_user_state(&self, user_id: &UserId) -> MonkestoResult<UserState>;

    async fn lookup_user_id(&self, username: &Email) -> MonkestoResult<Option<UserId>>;

    async fn get_email(&self, user_id: &UserId) -> MonkestoResult<Email> {
        Ok(self.get_user_state(user_id).await?.email)
    }

    async fn seed_user(
        &self,
        creation_event: UserEvent,
        update_events: Vec<UserEvent>,
    ) -> MonkestoResult<()> {
        if let UserEvent::Created { id, .. } = creation_event {
            self.create_user(creation_event).await?;

            for event in update_events {
                self.push_event(&id, event).await?;
            }
        } else {
            return Err(KnownErrors::IncorrectEventType);
        }

        Ok(())
    }
}

#[allow(dead_code)]
#[derive(Clone)]
pub struct UserMemoryStore {
    events: Arc<DashMap<UserId, Vec<UserEvent>>>,
    user_table: Arc<DashMap<UserId, UserState>>,
    email_lookup_table: Arc<DashMap<Email, UserId>>,
}

#[allow(dead_code)]
impl UserMemoryStore {
    pub fn new() -> Self {
        Self {
            events: Arc::new(DashMap::new()),
            user_table: Arc::new(DashMap::new()),
            email_lookup_table: Arc::new(DashMap::new()),
        }
    }
}

#[derive(Clone)]
pub struct Credentials {
    pub email: Email,
    pub password: String,
}

impl AuthnBackend for UserMemoryStore {
    type User = UserState;
    type Credentials = Credentials;
    type Error = KnownErrors;
    async fn authenticate(
        &self,
        creds: Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        if let Some(id) = self.email_lookup_table.get(&creds.email)
            && let Some(state) = self.user_table.get(&id)
        {
            let state_clone = state.value().clone();
            return task::spawn_blocking(move || {
                if bcrypt::verify(creds.password, &state_clone.pw_hash).is_ok_and(|p| p) {
                    Ok(Some(state_clone))
                } else {
                    Ok(None)
                }
            })
            .await?;
        }
        Ok(None)
    }

    async fn get_user(
        &self,
        user_id: &::axum_login::UserId<Self>,
    ) -> Result<Option<Self::User>, Self::Error> {
        Ok(self.user_table.get(user_id).map(|s| (*s).clone()))
    }
}

#[async_trait]
impl UserStore for UserMemoryStore {
    async fn create_user(&self, creation_event: UserEvent) -> MonkestoResult<()> {
        if let UserEvent::Created { id, email, pw_hash } = creation_event.clone() {
            let mut session_hash = [0u8; 16];
            OsRng
                .try_fill_bytes(&mut session_hash)
                .map_err(|e| KnownErrors::OsError {
                    context: e.to_string(),
                })?;

            let state = UserState {
                id,
                email: email.clone(),
                pw_hash,
                session_hash,
                deleted: false,
            };

            // insert the state into the user table
            self.user_table.insert(id, state);

            // insert the creation_event into the events table
            self.events.insert(id, vec![creation_event]);

            // insert the username and id into the username lookup table
            self.email_lookup_table.insert(email, id);

            Ok(())
        } else {
            Err(KnownErrors::IncorrectEventType)
        }
    }

    async fn push_event(&self, user_id: &UserId, event: UserEvent) -> MonkestoResult<()> {
        if let Some(mut events) = self.events.get_mut(user_id)
            && let Some(mut state) = self.user_table.get_mut(user_id)
        {
            state.apply(event.clone());
            events.push(event);
        } else {
            return Err(KnownErrors::UserDoesntExist);
        }

        Ok(())
    }

    async fn get_user_state(&self, user_id: &UserId) -> MonkestoResult<UserState> {
        self.user_table
            .get(user_id)
            .map(|state| (*state).clone())
            .ok_or(KnownErrors::UserDoesntExist)
    }

    async fn lookup_user_id(&self, email: &Email) -> MonkestoResult<Option<UserId>> {
        Ok(self.email_lookup_table.get(email).map(|id| *id))
    }
}

#[derive(Deserialize)]
pub struct SignupForm {
    email: String,
    password: String,
    confirm_password: String,
    next: Option<String>,
}

pub async fn create_user(
    State(state): State<AppState>,
    mut session: AuthSession,
    Form(form): Form<SignupForm>,
) -> Result<Redirect, Redirect> {
    const CALLBACK_URL: &str = "/signup";

    let email = Email::try_new(&form.email)
        .map_err(|_| KnownErrors::InvalidEmail { email: form.email })
        .or_redirect(CALLBACK_URL)?;

    if form.password != form.confirm_password {
        return Err(KnownErrors::SignupPasswordMismatch { email }.redirect(CALLBACK_URL));
    }

    if state
        .user_store
        .lookup_user_id(&email)
        .await
        .or_redirect(CALLBACK_URL)?
        .is_none()
    {
        let id = UserId::new();

        let pw_clone = form.password.clone();

        state
            .user_store
            .create_user(UserEvent::Created {
                id,
                email: email.clone(),
                pw_hash: spawn_blocking(move || bcrypt::hash(pw_clone, DEFAULT_COST))
                    .await
                    .or_redirect(CALLBACK_URL)?
                    .or_redirect(CALLBACK_URL)?,
            })
            .await
            .or_redirect(CALLBACK_URL)?;

        if let Ok(Some(user)) = session
            .authenticate(Credentials {
                email: email.clone(),
                password: form.password,
            })
            .await
        {
            if let Err(e) = session.login(&user).await {
                return Err(KnownErrors::InternalError {
                    context: e.to_string(),
                }
                .redirect(CALLBACK_URL));
            }
            Ok(Redirect::to(&form.next.unwrap_or(CALLBACK_URL.to_string())))
        } else {
            Err(KnownErrors::LoginFailed { email }.redirect("/login"))
        }
    } else {
        Err(KnownErrors::UserExists { email }.redirect(CALLBACK_URL))
    }
}

#[derive(Deserialize)]
pub struct LoginForm {
    email: String,
    password: String,
    next: Option<String>,
}

pub async fn login(
    mut session: AuthSession,
    Form(form): Form<LoginForm>,
) -> Result<Redirect, Redirect> {
    const CALLBACK_URL: &str = "/login";

    let email = Email::try_new(form.email)
        .map_err(|_| KnownErrors::UserDoesntExist)
        .or_redirect(CALLBACK_URL)?;

    if let Ok(Some(user)) = session
        .authenticate(Credentials {
            email: email.clone(),
            password: form.password,
        })
        .await
    {
        if let Err(e) = session.login(&user).await {
            return Err(KnownErrors::InternalError {
                context: e.to_string(),
            }
            .redirect(CALLBACK_URL));
        }
        Ok(Redirect::to(&form.next.unwrap_or("journal".to_string())))
    } else {
        Err(KnownErrors::LoginFailed { email }.redirect(CALLBACK_URL))
    }
}

pub async fn log_out(mut session: AuthSession) -> Result<Redirect, Redirect> {
    if let Err(e) = session.logout().await {
        Err(KnownErrors::InternalError {
            context: e.to_string(),
        }
        .redirect("/journal"))
    } else {
        Ok(Redirect::to("/login"))
    }
}
