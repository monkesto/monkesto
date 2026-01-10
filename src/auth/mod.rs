pub mod axum_login;
pub mod user;
pub mod username;
pub mod view;

use crate::auth::axum_login::{AuthSession, Credentials};
use crate::cuid::Cuid;
use crate::known_errors::MonkestoResult;
use crate::known_errors::{KnownErrors, RedirectOnError};
use async_trait::async_trait;
use axum::Extension;
use axum::extract::Form;
use axum::response::Redirect;
use serde::Deserialize;
use sqlx::PgPool;
use user::{UserEvent, UserState};

#[async_trait]
#[allow(dead_code)]
pub trait UserStore {
    /// adds a UserEvent to the event store and updates the cached state
    async fn push_event(&self, user_id: &Cuid, event: UserEvent) -> MonkestoResult<()>;

    async fn get_user(&self, user_id: &Cuid) -> MonkestoResult<UserState>;

    async fn lookup_user(&self, username: &str) -> MonkestoResult<UserState>;
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
