use super::return_types::KnownErrors;
use axum::Extension;
use leptos::prelude::ServerFnError;
use leptos_axum::extract;
use sqlx::PgPool;
use tower_sessions::Session;

pub async fn get_pool() -> Result<PgPool, ServerFnError> {
    if let Ok(Extension(s)) = extract::<Extension<PgPool>>().await {
        return Ok(s);
    }
    Err(ServerFnError::ServerError(
        "unable to fetch postgres pool".to_string(),
    ))
}

pub async fn get_session_id() -> Result<String, ServerFnError> {
    let Extension(session) = extract::<Extension<Session>>().await?;

    if session
        .get::<bool>("initialized")
        .await
        .ok()
        .flatten()
        .is_none()
    {
        _ = session.insert("initialized", true).await;
    }

    if let Some(s) = session.id() {
        return Ok(s.to_string());
    }
    Err(ServerFnError::ServerError(
        KnownErrors::SessionIdNotFound.to_string()?,
    ))
}
