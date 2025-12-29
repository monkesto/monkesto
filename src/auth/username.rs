use crate::api::extensions;
use crate::api::return_types::Cuid;
use crate::api::return_types::KnownErrors;
use crate::auth::user::get_user_id_from_session;
use leptos::prelude::ServerFnError;
use leptos::server;
use sqlx::PgPool;

pub async fn update(
    user_id: &Cuid,
    username: &String,
    pool: &PgPool,
) -> Result<i64, ServerFnError> {
    let id: i64 = sqlx::query_scalar(
        r#"
        INSERT INTO username_events (
        user_id,
        username
        )
        VALUES ($1, $2)
        RETURNING id
        "#,
    )
    .bind(user_id.to_bytes())
    .bind(username)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

pub async fn get_username(user_id: &Cuid, pool: &PgPool) -> Result<Option<String>, ServerFnError> {
    let username: Option<String> = sqlx::query_scalar(
        r#"
        SELECT username FROM username_events
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(user_id.to_bytes())
    .fetch_optional(pool)
    .await?;

    Ok(username)
}

pub async fn get_id(username: &String, pool: &PgPool) -> Result<Option<Cuid>, ServerFnError> {
    let id: Option<Vec<u8>> = sqlx::query_scalar(
        r#"
        SELECT user_id FROM username_events
        WHERE username = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(username)
    .fetch_optional(pool)
    .await?;

    if let Some(s) = id {
        Ok(Some(Cuid::from_bytes(&s)?))
    } else {
        Ok(None)
    }
}

#[server]
pub async fn get_username_from_session() -> Result<String, ServerFnError> {
    let pool = extensions::get_pool().await?;
    let user_id = get_user_id_from_session().await?;

    get_username(&user_id, &pool)
        .await?
        .ok_or(ServerFnError::ServerError(
            KnownErrors::UserDoesntExist.to_string()?,
        ))
}
