use axum::Extension;
use leptos::prelude::ServerFnError;
use leptos_axum::extract;
use sqlx::PgPool;
use tower_sessions::Session;

pub async fn get_pool() -> Result<PgPool, ServerFnError> {
    match extract::<Extension<PgPool>>().await {
        Ok(Extension(pool)) => Ok(pool),
        Err(e) => Err(ServerFnError::ServerError(e.to_string())),
    }
}

pub async fn get_session_id() -> Result<String, ServerFnError> {
    let Extension(session) = match extract::<Extension<Session>>().await {
        Ok(s) => s,
        Err(e) => return Err(ServerFnError::ServerError(e.to_string())),
    };

    if session
        .get::<bool>("initialized")
        .await
        .ok()
        .flatten()
        .is_none()
    {
        _ = session.insert("initialized", true).await;
    }

    match session.id() {
        Some(s) => Ok(s.to_string()),
        None => Err(ServerFnError::ServerError(
            "failed to fetch session id".to_string(),
        )),
    }
}
