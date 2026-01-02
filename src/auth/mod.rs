pub mod user;
pub mod username;
pub mod view;
use crate::cuid::Cuid;
use crate::extensions;
use crate::known_errors::{KnownErrors, RedirectOnError};
use axum::Extension;
use axum::extract::Form;
use axum::response::Redirect;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use leptos::prelude::ServerFnError;
use serde::Deserialize;
use sqlx::PgPool;
use tower_sessions::Session;
use user::UserEvent;

#[derive(sqlx::Type, PartialEq)]
#[sqlx(type_name = "smallint")]
#[repr(i16)]
pub enum AuthEvent {
    Login = 1,
    Logout = 2,
}

impl AuthEvent {
    pub async fn push_db(
        &self,
        user_id: &Cuid,
        session_id: &String,
        pool: &PgPool,
    ) -> Result<i64, ServerFnError> {
        let session_bytes = URL_SAFE_NO_PAD.decode(session_id)?;

        let id: i64 = sqlx::query_scalar(
            r#"
            INSERT INTO auth_events (
                user_id,
                session_id,
                event_type
            )
            VALUES ($1, $2, $3)
            RETURNING id
            "#,
        )
        .bind(user_id.to_bytes())
        .bind(session_bytes)
        .bind(self)
        .fetch_one(pool)
        .await?;

        Ok(id)
    }
}

pub async fn get_user_id(session_id: &str, pool: &PgPool) -> Result<Cuid, ServerFnError> {
    let session_bytes = URL_SAFE_NO_PAD.decode(session_id)?;

    let event: Vec<(Vec<u8>, AuthEvent)> = sqlx::query_as(
        r#"
        SELECT user_id, event_type FROM auth_events
        WHERE session_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(session_bytes)
    .fetch_all(pool)
    .await?;

    // check that a row with the session id exists
    let (id, auth_type) = match event.first() {
        Some(s) => s,
        None => {
            return Err(ServerFnError::ServerError(
                KnownErrors::NotLoggedIn.to_string()?,
            ));
        }
    };

    // if the latest event was a login, return the user id
    if *auth_type == AuthEvent::Login {
        return Cuid::from_bytes(id);
    }

    Err(ServerFnError::ServerError(
        KnownErrors::NotLoggedIn.to_string()?,
    ))
}

#[derive(Deserialize)]
pub struct SignupForm {
    username: String,
    password: String,
    confirm_password: String,
}

pub async fn create_user(
    Extension(pool): Extension<PgPool>,
    session: Session,
    Form(form): Form<SignupForm>,
) -> Result<Redirect, Redirect> {
    let session_id = extensions::intialize_session(&session)
        .await
        .or_redirect(KnownErrors::SessionIdNotFound, "/signup")?;

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
        .or_redirect(
            KnownErrors::InternalError {
                context: "fetching username from the database".to_string(),
            },
            "/signup",
        )?
        .is_none()
    {
        let id = Cuid::new16();

        UserEvent::Created {
            hashed_password: bcrypt::hash(form.password, bcrypt::DEFAULT_COST).or_redirect(
                KnownErrors::InternalError {
                    context: "hashing password".to_string(),
                },
                "/signup",
            )?,
        }
        .push_db(&id, &pool)
        .await
        .or_redirect(
            KnownErrors::InternalError {
                context: "pushing user creation event".to_string(),
            },
            "/signup",
        )?;

        username::update(&id, &form.username, &pool)
            .await
            .or_redirect(
                KnownErrors::InternalError {
                    context: "pushing username update".to_string(),
                },
                "/signup",
            )?;

        AuthEvent::Login
            .push_db(&id, &session_id, &pool)
            .await
            .or_redirect(
                KnownErrors::LoginFailed {
                    username: form.username,
                },
                "/login",
            )?;
    } else {
        return Err(KnownErrors::UserExists {
            username: form.username,
        }
        .redirect("/signup"));
    }

    Ok(Redirect::to("/journal"))
}

#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

pub async fn login(
    Extension(pool): Extension<PgPool>,
    session: Session,
    Form(form): Form<LoginForm>,
) -> Result<Redirect, Redirect> {
    let session_id = extensions::intialize_session(&session)
        .await
        .or_redirect(KnownErrors::SessionIdNotFound, "/login")?;

    let user_id = match username::get_id(&form.username, &pool).await {
        Ok(Some(s)) => s,
        Ok(None) => return Err(KnownErrors::UserDoesntExist.redirect("/login")),
        Err(_) => {
            return Err(KnownErrors::InternalError {
                context: "fetching username".to_string(),
            }
            .redirect("/login"));
        }
    };

    let hashed_password = user::get_hashed_pw(&user_id, &pool).await.or_redirect(
        KnownErrors::InternalError {
            context: "fetching password".to_string(),
        },
        "/login",
    )?;

    if bcrypt::verify(&form.password, &hashed_password).is_ok_and(|f| f) {
        AuthEvent::Login
            .push_db(&user_id, &session_id, &pool)
            .await
            .or_redirect(
                KnownErrors::InternalError {
                    context: "pushing login event".to_string(),
                },
                "/login",
            )?;

        Ok(Redirect::to("/journal"))
    } else {
        Err(KnownErrors::LoginFailed {
            username: form.username,
        }
        .redirect("/login"))
    }
}

pub async fn log_out(
    Extension(pool): Extension<PgPool>,
    session: Session,
) -> Result<Redirect, Redirect> {
    // if the user tries to enter this page without being logged in, silently redirect to the login page
    let session_id = extensions::intialize_session(&session)
        .await
        .or_redirect(KnownErrors::None, "/login")?;

    let user_id = get_user_id(&session_id, &pool)
        .await
        .or_redirect(KnownErrors::None, "/login")?;

    AuthEvent::Logout
        .push_db(&user_id, &session_id, &pool)
        .await
        .or_redirect(
            KnownErrors::InternalError {
                context: "pushing logout evenet".to_string(),
            },
            "/journal",
        )?;

    Ok(Redirect::to("/login"))
}
