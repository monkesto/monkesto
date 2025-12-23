use leptos::prelude::ServerFnError;
use sqlx::PgPool;
use uuid::Uuid;

pub async fn update(
    user_id: &Uuid,
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
    .bind(user_id)
    .bind(username)
    .fetch_one(pool)
    .await?;

    Ok(id)
}

pub async fn get_username(user_id: &Uuid, pool: &PgPool) -> Result<Option<String>, ServerFnError> {
    let username: Option<String> = sqlx::query_scalar(
        r#"
        SELECT username FROM username_events
        WHERE user_id = $1
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(username)
}

pub async fn get_id(username: &String, pool: &PgPool) -> Result<Option<Uuid>, ServerFnError> {
    let id: Option<Uuid> = sqlx::query_scalar(
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

    Ok(id)
}
