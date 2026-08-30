use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::journal::file::{FileId, ObjectStore};
use crate::journal::{JournalError, JournalId, Permissions};
use crate::monkesto_error::OrRedirect;
use crate::time_provider::{DefaultTimeProvider, TimeProvider};
use crate::{BackendType, StateType};
use aws_sdk_s3::presigning::PresigningConfig;
use axum::Json;
use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{Request, StatusCode};
use axum::response::Redirect;
use axum_login::AuthSession;
use axum_test::expect_json::__private::serde_trampoline::Deserialize;
use futures_util::stream::StreamExt;
use serde::Serialize;
use std::io::Read;
use std::str::FromStr;
use std::time::Duration;
use tokio::fs::{File, create_dir_all};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;
    let user = get_user(session)?;
    let user_authority = Authority::Direct(Actor::User(user.id));

    let file_id = FileId::new();
    let file_key = format!("{}/{}-{}", journal_id, file_id, form.file_name);

    let upload_url = match state.journal_service.object_store.clone() {
        ObjectStore::S3 {
            s3_client,
            bucket_name,
        } => {
            let presign_ttl = Duration::from_secs(60);

            let presign_config = PresigningConfig::expires_in(presign_ttl).expect("valid ttl");

            let presigned_req = s3_client
                .put_object()
                .bucket(bucket_name)
                .key(&file_key)
                .content_length(form.file_size)
                .presigned(presign_config)
                .await
                .map_err(JournalError::from)
                .or_redirect(callback_url)?;

            presigned_req.uri().to_string()
        }
        ObjectStore::Local { .. } => {
            format!(
                "/journal/{}/file/localupload?file_key={}",
                journal_id, file_key
            )
        }
    };

    if state
        .journal_service
        .get_effective_permissions(journal_id, user_authority)
        .await
        .or_redirect(callback_url)?
        .contains(Permissions::UPLOAD_FILE)
    {
        Ok(Json(UploadLink {
            upload_url,
            file_key,
        }))
    } else {
        Err(JournalError::Permissions(Permissions::UPLOAD_FILE)).or_redirect(callback_url)?
    }
}

#[derive(Deserialize)]
pub struct LocalUploadQuery {
    file_key: String,
}

pub async fn localstore_file_handler(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path(id): Path<String>,
    Query(query): Query<LocalUploadQuery>,
    request: Request<Body>,
) -> Result<StatusCode, Redirect> {
    let callback_url = &format!("/journal/{}/file", id);

    if let ObjectStore::Local { storage_directory } = state.journal_service.object_store.clone() {
        let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;
        let user = get_user(session)?;
        let user_authority = Authority::Direct(Actor::User(user.id));

        if state
            .journal_service
            .get_effective_permissions(journal_id, user_authority)
            .await
            .or_redirect(callback_url)?
            .contains(Permissions::UPLOAD_FILE)
        {
            let file_path = storage_directory.join(query.file_key);

            if let Some(parent) = file_path.parent() {
                create_dir_all(parent)
                    .await
                    .map_err(|e| JournalError::S3(e.to_string()))
                    .or_redirect(callback_url)?;
            }

            let mut file = File::create(file_path)
                .await
                .map_err(|e| JournalError::S3(e.to_string()))
                .or_redirect(callback_url)?;

            let mut request_stream = request.into_body().into_data_stream();
            while let Some(chunk_result) = request_stream.next().await {
                let chunk = chunk_result
                    .map_err(|e| JournalError::S3(e.to_string()))
                    .or_redirect(callback_url)?;
                file.write_all(&chunk)
                    .await
                    .map_err(|e| JournalError::S3(e.to_string()))
                    .or_redirect(callback_url)?;
            }

            file.flush()
                .await
                .map_err(|e| JournalError::S3(e.to_string()))
                .or_redirect(callback_url)?;

            Ok(StatusCode::OK)
        } else {
            Err(JournalError::Permissions(Permissions::UPLOAD_FILE)).or_redirect(callback_url)?
        }
    } else {
        Err(JournalError::S3("bad request".to_string())).or_redirect(callback_url)?
    }
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

    let hash = match state.journal_service.object_store.clone() {
        ObjectStore::S3 {
            s3_client,
            bucket_name,
        } => {
            let head = s3_client
                .head_object()
                .bucket(bucket_name)
                .key(&form.file_key)
                .send()
                .await
                .map_err(JournalError::from)
                .or_redirect(callback_url)?;

            let hash = head
                .e_tag()
                .ok_or_else(|| JournalError::S3("no MD5 checksum found".to_string()))
                .or_redirect(callback_url)?
                .trim_matches('"');

            hex::decode(hash)
                .map_err(|e| JournalError::S3(format!("failed to decode file hash: {}", e)))
                .or_redirect(callback_url)?
        }
        ObjectStore::Local { storage_directory } => {
            let mut file = File::open(storage_directory.join(&form.file_key))
                .await
                .map_err(|e| JournalError::S3(e.to_string()))
                .or_redirect(callback_url)?;

            let mut buf = Vec::new();

            file.read_to_end(&mut buf)
                .await
                .map_err(|e| JournalError::S3(e.to_string()))
                .or_redirect(callback_url)?;

            let digest = md5::compute(buf);

            digest.to_vec()
        }
    };

    let file_id =
        FileId::from_str(k[0].splitn(2, '/').collect::<Vec<_>>()[1]).or_redirect(callback_url)?;

    let journal_id = JournalId::from_str(&id).or_redirect(callback_url)?;

    let file_name = k[1];

    let user = get_user(session)?;
    let user_authority = Authority::Direct(Actor::User(user.id));

    let event_id = state
        .journal_service
        .upload_file(
            file_id,
            journal_id,
            *hash
                .as_array::<16>()
                .ok_or_else(|| {
                    JournalError::S3("incorrect file hash length, expected 16 bytes".to_string())
                })
                .or_redirect(callback_url)?,
            file_name.to_string(),
            user_authority,
            DefaultTimeProvider.get_time(),
        )
        .await
        .or_redirect(callback_url)?;

    state.journal_service.wait_for(event_id).await;

    Ok(Redirect::to(callback_url))
}
