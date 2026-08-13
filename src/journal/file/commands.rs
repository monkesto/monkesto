use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::journal::file::{FileId, S3_CLIENT};
use crate::journal::{JournalError, JournalId, Permissions};
use crate::monkesto_error::OrRedirect;
use crate::time_provider::{DefaultTimeProvider, TimeProvider};
use crate::{BackendType, StateType};
use aws_sdk_s3::presigning::PresigningConfig;
use aws_sdk_s3::types::ChecksumMode;
use axum::Json;
use axum::extract::{Path, State};
use axum::response::Redirect;
use axum_extra::extract::Form;
use axum_login::AuthSession;
use axum_test::expect_json::__private::serde_trampoline::Deserialize;
use serde::Serialize;
use std::str::FromStr;
use std::time::Duration;

#[derive(Deserialize)]
pub struct UploadFileForm {
    file_name: String,
    file_size: i64,
}

#[derive(Serialize)]
pub struct UploadLink {
    upload_url: String,
    file_key: String,
}

pub async fn upload_file(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Json(form): Json<UploadFileForm>,
) -> Result<Json<UploadLink>, Redirect> {
    let callback_url = &format!("/journal/{}/file", id);

    if form.file_size > 1024 * 1024 * 50 {
        return Err(JournalError::S3("File too large! 50 MB max".to_string()))
            .or_redirect(callback_url);
    }

    if let Some(s3_client) = S3_CLIENT.clone() {
        let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;

        let user = get_user(session)?;

        let user_authority = Authority::Direct(Actor::User(user.id));

        let file_id = FileId::new();

        let file_key = format!("{}/{}-{}", journal_id, file_id, form.file_name);

        let presign_ttl = Duration::from_secs(60);

        let presign_config = PresigningConfig::expires_in(presign_ttl).expect("valid ttl");

        let presigned_req = s3_client
            .put_object()
            .bucket("monkesto")
            .key(&file_key)
            .content_length(form.file_size)
            .presigned(presign_config)
            .await
            .map_err(JournalError::from)
            .or_redirect(callback_url)?;

        if state
            .journal_service
            .get_effective_permissions(journal_id, user_authority)
            .await
            .or_redirect(callback_url)?
            .contains(Permissions::UPLOAD_FILE)
        {
            return Ok(Json(UploadLink {
                upload_url: presigned_req.uri().to_string(),
                file_key,
            }));
        }

        return Err(JournalError::Permissions(Permissions::UPLOAD_FILE))
            .or_redirect(callback_url)?;
    }

    Err(JournalError::InvalidS3Credentials).or_redirect(callback_url)?
}

#[derive(Deserialize)]
pub struct RecordFileUploadForm {
    file_key: String,
}

pub async fn record_file_upload(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Json(form): Json<RecordFileUploadForm>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/file", id);

    let k = form.file_key.splitn(2, '-').collect::<Vec<_>>();

    if let Some(s3_client) = S3_CLIENT.clone() {
        let head = s3_client
            .head_object()
            .bucket("monkesto")
            .key(&form.file_key)
            .send()
            .await
            .map_err(JournalError::from)
            .or_redirect(callback_url)?;

        // the first slice should be in the format "{journal_id}/{file_id}"
        // using direct indexes is safe here because the key is set by us rather than the user

        let file_id = FileId::from_str(k[0].splitn(2, '/').collect::<Vec<_>>()[1])
            .or_redirect(callback_url)?;

        let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;

        let hash = head
            .e_tag()
            .ok_or_else(|| JournalError::S3("no MD5 checksum found".to_string()))
            .or_redirect(callback_url)?
            .trim_matches('"');
        let hash_bytes = hex::decode(hash)
            .map_err(|e| JournalError::S3(format!("failed to decode file hash: {}", e)))
            .or_redirect(callback_url)?;

        let file_name = k[1];

        let user = get_user(session)?;
        let user_authority = Authority::Direct(Actor::User(user.id));

        let event_id = state
            .journal_service
            .upload_file(
                file_id,
                journal_id,
                *hash_bytes
                    .as_array::<16>()
                    .ok_or_else(|| {
                        JournalError::S3(
                            "incorrect file hash length, expected 16 bytes".to_string(),
                        )
                    })
                    .or_redirect(callback_url)?,
                file_name.to_string(),
                user_authority,
                DefaultTimeProvider.get_time(),
            )
            .await
            .or_redirect(callback_url)?;

        state.journal_service.wait_for(event_id).await;

        return Ok(Redirect::to(callback_url));
    }

    Err(JournalError::InvalidS3Credentials).or_redirect(callback_url)?
}
