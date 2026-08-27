pub mod commands;
pub mod views;

use crate::authority::Authority;
use crate::event_id::GetEventId;
use crate::id;
use crate::id::Ident;
use crate::journal::domain::{FileEvent, JournalDomainEvent, JournalEvent};
use crate::journal::{
    Journal, JournalError, JournalId, JournalResult, JournalService, Permissions,
};
use crate::status::Status;
use crate::time_provider::Timestamp;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::config::Credentials;
use aws_types::region::Region;
use disintegrate::{Decision, DecisionError, StateMutate, StateQuery};
use disintegrate_postgres::PgEventId;
use prost::Message;
use proto::event::journal::ProtoJournalDomainEvent;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::env;
use std::sync::LazyLock;
use tokio::runtime::Handle;

id!(FileId, Ident::new16());

static S3_CLIENT: LazyLock<Option<S3Client>> = LazyLock::new(|| {
    if let Some(endpoint) = env::var("S3_ENDPOINT").ok()
        && let Some(access_key_id) = env::var("S3_ACCESS_KEY_ID").ok()
        && let Some(secret_access_key) = env::var("S3_SECRET_ACCESS_KEY").ok()
    {
        let credentials = Credentials::new(access_key_id, secret_access_key, None, None, "custom");

        let config = tokio::task::block_in_place(|| {
            Handle::current().block_on(async move {
                aws_config::defaults(aws_config::BehaviorVersion::latest())
                    .endpoint_url(endpoint)
                    .region(Region::from_static("auto"))
                    .credentials_provider(credentials)
                    .load()
                    .await
            })
        });
        return Some(S3Client::new(&config));
    }
    None
});

#[derive(StateQuery, Clone, Default, Serialize, Deserialize)]
#[state_query(FileEvent)]
pub struct FileUpload {
    #[id]
    pub file_id: FileId,
    #[id]
    pub journal_id: JournalId,
    pub hash: [u8; 16],
    file_name: String,
    status: Status,
}

impl StateMutate for FileUpload {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            FileEvent::FileUploaded {
                file_id,
                journal_id,
                hash,
                file_name,
                ..
            } => {
                self.file_id = file_id;
                self.journal_id = journal_id;
                self.hash = hash;
                self.file_name = file_name;
            }
        }
    }
}

impl FileUpload {
    fn new(file_id: FileId) -> Self {
        Self {
            file_id,
            ..Default::default()
        }
    }
}

/// The user should verify that an uploaded file exists with a matching hash before sending this event
pub struct UploadFile {
    file_id: FileId,
    journal_id: JournalId,
    hash: [u8; 16],
    file_name: String,
    authority: Authority,
    timestamp: Timestamp,
}

impl UploadFile {
    pub fn new(
        file_id: FileId,
        journal_id: JournalId,
        hash: [u8; 16],
        file_name: String,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            file_id,
            journal_id,
            hash,
            file_name,
            authority,
            timestamp,
        }
    }
}

impl Decision for UploadFile {
    type Event = JournalDomainEvent;
    type StateQuery = (FileUpload, Journal);
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        (FileUpload::new(self.file_id), Journal::new(self.journal_id))
    }

    fn process(
        &self,
        (upload_state, journal_state): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        let file_path = format!("{}/{}-{}", self.journal_id, self.file_id, self.file_name);

        if upload_state.status.found() {
            // attempt to delete the uploaded file and make the user try again

            if let Some(s3_client) = S3_CLIENT.clone() {
                tokio::spawn(async move {
                    _ = s3_client
                        .delete_object()
                        .bucket("monkesto")
                        .key(&file_path)
                        .send()
                        .await
                });
            }

            return Err(JournalError::FileIdCollision(upload_state.file_id));
        };

        if !journal_state.status.valid() {
            return Err(JournalError::InvalidJournal(journal_state.journal_id));
        }

        Ok(vec![JournalDomainEvent::FileUploaded {
            file_id: self.file_id,
            journal_id: self.journal_id,
            hash: self.hash,
            file_name: self.file_name.clone(),
            authority: self.authority,
            timestamp: self.timestamp,
        }])
    }
}

pub struct FileState {
    pub id: FileId,
    #[expect(unused)]
    journal_id: JournalId,
    #[expect(unused)]
    pub hash: [u8; 16],
    pub name: String,
}

#[derive(FromRow)]
struct FileStateWithPayload {
    id: FileId,
    #[expect(unused)]
    journal_id: JournalId,
    hash: Vec<u8>,
    name: String,
    payload: Vec<u8>,
}

#[derive(FromRow)]
struct FileStateWithVecHash {
    id: FileId,
    journal_id: JournalId,
    hash: Vec<u8>,
    name: String,
}

impl JournalService {
    pub async fn upload_file(
        &self,
        file_id: FileId,
        journal_id: JournalId,
        hash: [u8; 16],
        file_name: String,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(UploadFile::new(
                file_id, journal_id, hash, file_name, authority, timestamp,
            ))
            .await?
            .event_id())
    }

    pub async fn get_file(
        &self,
        file_id: FileId,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<FileState> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(JournalError::InvalidJournal(journal_id));
        }

        let file = sqlx::query_as!(
            FileStateWithVecHash,
            r#"
            SELECT f.id as "id: FileId", f.journal_id as "journal_id: JournalId", f.hash, f.name
            FROM files f
            WHERE f.journal_id = $1 AND f.id = $2
            "#,
            journal_id as JournalId,
            file_id as FileId,
        )
        .fetch_optional(&self.projection_pool)
        .await?;

        if let Some(file) = file {
            let hash = *file.hash.as_array::<16>().ok_or_else(|| {
                JournalError::EventDecode(format!(
                    "expected 16 byte file hash, got {}",
                    file.hash.len()
                ))
            })?;

            return Ok(FileState {
                id: file.id,
                journal_id: file.journal_id,
                hash,
                name: file.name,
            });
        }

        Err(JournalError::InvalidFile(file_id))
    }

    pub async fn list_journal_files(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(FileState, Authority, Timestamp)>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(JournalError::InvalidJournal(journal_id));
        }

        let files = sqlx::query_as!(
            FileStateWithPayload,
            r#"
            SELECT f.id as "id: FileId", f.journal_id as "journal_id: JournalId", f.hash, f.name, e.payload as "payload!"
            FROM files f
            INNER JOIN event e
                ON e.file_id = f.id AND e.event_type = 'FileUploaded'
            WHERE f.journal_id = $1
            "#,
            journal_id as JournalId
        )
            .fetch_all(&self.projection_pool)
            .await?;

        let mut files_with_meta = Vec::with_capacity(files.len());

        for file in files {
            let payload = JournalDomainEvent::try_from(ProtoJournalDomainEvent::decode(
                file.payload.as_slice(),
            )?)?;
            let hash = *file.hash.as_array::<16>().ok_or_else(|| {
                JournalError::EventDecode(format!(
                    "expected 16 byte file hash, got {}",
                    file.hash.len()
                ))
            })?;

            match payload {
                JournalDomainEvent::FileUploaded {
                    authority,
                    timestamp,
                    ..
                } => files_with_meta.push((
                    FileState {
                        id: file.id,
                        journal_id,
                        hash,
                        name: file.name,
                    },
                    authority,
                    timestamp,
                )),
                _ => unreachable!("FileUploaded events are filtered by the sql query"),
            }
        }

        Ok(files_with_meta)
    }
}
