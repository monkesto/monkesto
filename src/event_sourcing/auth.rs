use crate::api::return_types::Cuid;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use leptos::prelude::ServerFnError;
use sqlx::PgPool;

use crate::api::return_types::KnownErrors;

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
