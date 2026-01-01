pub mod user;
pub mod username;
pub mod view;
use crate::cuid::Cuid;
use crate::extensions;
use crate::known_errors::{KnownErrors, error_message, return_error};
use crate::ok_or_return_error;
use axum::Extension;
use axum::extract::Form;
use axum::response::{IntoResponse, Redirect, Response};
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
) -> Response {
    let session_id = ok_or_return_error!(
        extensions::intialize_session(&session).await,
        "fetching session id"
    );

    if form.username.trim().is_empty() {
        return error_message("your username can not be empty");
    }

    if form.password != form.confirm_password {
        return error_message("your passwords do not match");
    }

    if ok_or_return_error!(
        username::get_id(&form.username, &pool).await,
        "fetching user id"
    )
    .is_none()
    {
        let id = Cuid::new16();

        ok_or_return_error!(
            UserEvent::Created {
                hashed_password: ok_or_return_error!(
                    bcrypt::hash(form.password, bcrypt::DEFAULT_COST),
                    "hashing password"
                ),
            }
            .push_db(&id, &pool)
            .await,
            "creating user"
        );

        ok_or_return_error!(
            username::update(&id, &form.username, &pool).await,
            "updating username"
        );

        ok_or_return_error!(
            AuthEvent::Login.push_db(&id, &session_id, &pool).await,
            "logging in"
        );
    } else {
        return error_message(&format!(
            "the username \"{}\" is not available ",
            form.username
        ));
    }

    Redirect::to("/").into_response()
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
) -> impl IntoResponse {
    let session_id = ok_or_return_error!(
        extensions::intialize_session(&session).await,
        "fetching session id"
    );

    let user_id = match username::get_id(&form.username, &pool).await {
        Ok(Some(s)) => s,
        Ok(None) => return error_message("invalid username!"),
        Err(e) => return return_error(e, "fetching username"),
    };

    let hashed_password = ok_or_return_error!(
        user::get_hashed_pw(&user_id, &pool).await,
        "fetching the user's password"
    );

    if bcrypt::verify(&form.password, &hashed_password).is_ok_and(|f| f) {
        ok_or_return_error!(
            AuthEvent::Login.push_db(&user_id, &session_id, &pool).await,
            "logging in"
        );

        Redirect::to("/").into_response()
    } else {
        error_message("failed to log in: incorrect password")
    }
}

pub async fn log_out(Extension(pool): Extension<PgPool>, session: Session) -> Response {
    let session_id = ok_or_return_error!(
        extensions::intialize_session(&session).await,
        "fetching session id"
    );

    let user_id = ok_or_return_error!(get_user_id(&session_id, &pool).await, "fetching user id");

    ok_or_return_error!(
        AuthEvent::Logout
            .push_db(&user_id, &session_id, &pool)
            .await,
        "logging out"
    );

    Redirect::to("/login").into_response()
}
