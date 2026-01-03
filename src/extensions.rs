use super::known_errors::KnownErrors;
use tower_sessions::Session;

pub async fn intialize_session(session: &Session) -> Result<String, KnownErrors> {
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
    Err(KnownErrors::SessionIdNotFound)
}
