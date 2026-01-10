pub mod axum_login;
pub mod user;
pub mod username;
pub mod view;

use std::collections::HashSet;
use std::sync::Arc;

use crate::auth::axum_login::{AuthSession, Credentials};
use crate::cuid::Cuid;
use crate::known_errors::MonkestoResult;
use crate::known_errors::{KnownErrors, RedirectOnError};
use async_trait::async_trait;
use axum::Extension;
use axum::extract::Form;
use axum::response::Redirect;
use dashmap::DashMap;
use serde::Deserialize;
use sqlx::PgPool;
use user::{UserEvent, UserState};

#[async_trait]
#[allow(dead_code)]
pub trait UserStore: Send + Sync {
    /// creates a new user state in the event store with the data from the creation event
    ///
    /// it should return an error if the event passed in is not a creation event
    async fn create_user(&self, creation_event: UserEvent) -> MonkestoResult<()>;

    /// adds a UserEvent to the event store and updates the cached state
    async fn push_event(&self, user_id: &Cuid, event: UserEvent) -> MonkestoResult<()>;

    async fn get_user_state(&self, user_id: &Cuid) -> MonkestoResult<UserState>;

    async fn lookup_user_id(&self, username: &str) -> MonkestoResult<Option<Cuid>>;

    async fn get_pending_journals(&self, user_id: &Cuid) -> MonkestoResult<HashSet<Cuid>> {
        Ok(self.get_user_state(user_id).await?.pending_journal_invites)
    }

    async fn get_associated_journals(&self, user_id: &Cuid) -> MonkestoResult<HashSet<Cuid>> {
        Ok(self.get_user_state(user_id).await?.associated_journals)
    }
}

#[allow(dead_code)]
pub struct UserMemoryStore {
    events: Arc<DashMap<Cuid, Vec<UserEvent>>>,
    user_table: Arc<DashMap<Cuid, UserState>>,
    username_lookup_table: Arc<DashMap<String, Cuid>>,
}

#[allow(dead_code)]
impl UserMemoryStore {
    pub fn new() -> Self {
        Self {
            events: Arc::new(DashMap::new()),
            user_table: Arc::new(DashMap::new()),
            username_lookup_table: Arc::new(DashMap::new()),
        }
    }
}

#[async_trait]
impl UserStore for UserMemoryStore {
    async fn create_user(&self, creation_event: UserEvent) -> MonkestoResult<()> {
        if let UserEvent::Created { id, username } = creation_event.clone() {
            let state = UserState {
                id,
                username: username.clone(),
                pending_journal_invites: HashSet::new(),
                associated_journals: HashSet::new(),
                deleted: false,
            };

            // insert the state into the user table
            self.user_table.insert(id, state);

            // insert the creation_event into the events table
            self.events.insert(id, vec![creation_event]);

            // insert the username and id into the username lookup table
            self.username_lookup_table.insert(username, id);

            Ok(())
        } else {
            Err(KnownErrors::IncorrectEventType)
        }
    }

    async fn push_event(&self, user_id: &Cuid, event: UserEvent) -> MonkestoResult<()> {
        if let Some(mut events) = self.events.get_mut(user_id)
            && let Some(mut state) = self.user_table.get_mut(user_id)
        {
            if let UserEvent::Renamed { name } = event.clone() {
                if self.username_lookup_table.get(&state.username).is_some() {
                    self.username_lookup_table.remove(&state.username);
                    self.username_lookup_table.insert(name, *user_id);
                } else {
                    return Err(KnownErrors::UserDoesntExist);
                }
            }
            state.apply(event.clone());
            events.push(event);
        } else {
            return Err(KnownErrors::UserDoesntExist);
        }

        Ok(())
    }

    async fn get_user_state(&self, user_id: &Cuid) -> MonkestoResult<UserState> {
        self.user_table
            .get(user_id)
            .map(|state| (*state).clone())
            .ok_or(KnownErrors::UserDoesntExist)
    }

    async fn lookup_user_id(&self, username: &str) -> MonkestoResult<Option<Cuid>> {
        Ok(self.username_lookup_table.get(username).map(|id| *id))
    }
}

#[allow(dead_code)]
pub struct Users {
    store: dyn UserStore,
}

#[derive(Deserialize)]
pub struct SignupForm {
    username: String,
    password: String,
    confirm_password: String,
    next: Option<String>,
}

pub async fn create_user(
    Extension(pool): Extension<PgPool>,
    mut session: AuthSession,
    Form(form): Form<SignupForm>,
) -> Result<Redirect, Redirect> {
    if form.username.trim().is_empty() {
        return Err(KnownErrors::InvalidUsername {
            username: form.username,
        }
        .redirect("/signup"));
    }

    if form.password != form.confirm_password {
        return Err(KnownErrors::SignupPasswordMismatch {
            username: form.username,
        }
        .redirect("/signup"));
    }

    if username::get_id(&form.username, &pool)
        .await
        .or_redirect("/signup")?
        .is_none()
    {
        let id = Cuid::new16();
        let password_clone = form.password.clone();

        let pw_hash =
            tokio::task::spawn_blocking(|| bcrypt::hash(password_clone, bcrypt::DEFAULT_COST))
                .await
                .or_redirect("/signup")?
                .or_redirect("/signup")?;

        session
            .backend
            .create_auth_user(id, form.username.clone(), pw_hash)
            .await
            .or_redirect("/signup")?;

        username::update(&id, &form.username, &pool)
            .await
            .or_redirect("/signup")?;

        if let Ok(Some(user)) = session
            .authenticate(Credentials {
                username: form.username.clone(),
                password: form.password,
            })
            .await
        {
            if let Err(e) = session.login(&user).await {
                return Err(KnownErrors::InternalError {
                    context: e.to_string(),
                }
                .redirect("/login"));
            }
            Ok(Redirect::to(&form.next.unwrap_or("/journal".to_string())))
        } else {
            Err(KnownErrors::LoginFailed {
                username: form.username,
            }
            .redirect("/login"))
        }
    } else {
        Err(KnownErrors::UserExists {
            username: form.username,
        }
        .redirect("/signup"))
    }
}

#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
    next: Option<String>,
}

pub async fn login(
    mut session: AuthSession,
    Form(form): Form<LoginForm>,
) -> Result<Redirect, Redirect> {
    if let Ok(Some(user)) = session
        .authenticate(Credentials {
            username: form.username.clone(),
            password: form.password,
        })
        .await
    {
        if let Err(e) = session.login(&user).await {
            return Err(KnownErrors::InternalError {
                context: e.to_string(),
            }
            .redirect("/login"));
        }
        Ok(Redirect::to(&form.next.unwrap_or("/journal".to_string())))
    } else {
        Err(KnownErrors::LoginFailed {
            username: form.username,
        }
        .redirect("/login"))
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
