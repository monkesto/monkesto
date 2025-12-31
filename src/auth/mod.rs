pub mod user;
pub mod username;
pub mod view;
use crate::cuid::Cuid;
use crate::extensions::{self, get_pool, get_session_id};
use crate::known_errors::{KnownErrors, error_message, return_error};
use axum::Extension;
use axum::extract::Query;
use axum::response::{IntoResponse, Redirect};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use leptos::prelude::ServerFnError;
use leptos::server;
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

pub async fn get_user_id(session_id: &String, pool: &PgPool) -> Result<Cuid, ServerFnError> {
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

#[server]
pub async fn create_user(
    username: String,
    password: String,
    confirm_password: String,
) -> Result<(), ServerFnError> {
    let pool = get_pool().await?;
    let session_id = get_session_id().await?;

    if username.trim().is_empty() {
        return Err(ServerFnError::ServerError(
            KnownErrors::InvalidInput.to_string()?,
        ));
    }

    if password != confirm_password {
        return Err(ServerFnError::ServerError(
            KnownErrors::SignupPasswordMismatch { username }.to_string()?,
        ));
    }

    if username::get_id(&username, &pool).await?.is_none() {
        let id = Cuid::new16();
        UserEvent::Created {
            hashed_password: bcrypt::hash(password, bcrypt::DEFAULT_COST)?,
        }
        .push_db(&id, &pool)
        .await?;

        username::update(&id, &username, &pool).await?;

        AuthEvent::Login.push_db(&id, &session_id, &pool).await?;
    } else {
        return Err(ServerFnError::ServerError(
            KnownErrors::UserExists { username }.to_string()?,
        ));
    }

    Ok(())
}

#[derive(Deserialize)]
pub struct LoginForm {
    username: String,
    password: String,
}

pub async fn login(
    Query(form): Query<LoginForm>,
    Extension(pool): Extension<PgPool>,
    session: Session,
) -> impl IntoResponse {
    let session_id = match extensions::intialize_session(&session).await {
        Ok(s) => s,
        Err(e) => return return_error(e, "fetching session id"),
    };

    let user_id = match username::get_id(&form.username, &pool).await {
        Ok(Some(s)) => s,
        Ok(None) => return error_message("invalid username!"),
        Err(e) => return return_error(e, "fetching username"),
    };

    let hashed_password = match user::get_hashed_pw(&user_id, &pool).await {
        Ok(s) => s,
        Err(e) => return return_error(e, "fetching password"),
    };

    if bcrypt::verify(&form.password, &hashed_password).is_ok_and(|f| f) {
        let login_res = AuthEvent::Login.push_db(&user_id, &session_id, &pool).await;
        if let Err(e) = login_res {
            return return_error(e, "logging in");
        }

        Redirect::to("/").into_response()
    } else {
        error_message("failed to log in: incorrect password")
    }
}

#[server]
pub async fn log_out() -> Result<(), ServerFnError> {
    let session_id = get_session_id().await?;
    let pool = get_pool().await?;

    let user_id = get_user_id(&session_id, &pool).await?;

    AuthEvent::Logout
        .push_db(&user_id, &session_id, &pool)
        .await?;
    Ok(())
}
