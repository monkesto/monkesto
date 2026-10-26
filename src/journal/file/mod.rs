pub mod commands;
pub mod views;

use crate::authority::Authority;
use crate::event_id::GetEventId;
use crate::id;
use crate::id::Ident;
use crate::journal::error::JournalError::InvalidJournal;
use crate::journal::error::{JournalError, JournalResult};
use crate::journal::event::JournalDomainEvent;
use crate::journal::service::stream_lock::{
    append_creation_event_in_tx, append_modification_event_in_tx, get_stream_handle,
};
use crate::journal::service::{EventId, EventType, JournalService, StreamType};
use crate::journal::{JournalId, Permissions};
use crate::time::Timestamp;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::config::Credentials;
use aws_sdk_s3::types::{CorsConfiguration, CorsRule};
use aws_types::region::Region;
use prost::Message;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use std::env;
use std::path::PathBuf;
use tokio::fs::create_dir_all;

id!(FileId, Ident::new16());

#[derive(Clone)]
pub enum ObjectStore {
    S3 {
        s3_client: S3Client,
        bucket_name: String,
    },
    Local {
        storage_directory: PathBuf,
    },
}

impl ObjectStore {
    pub async fn new() -> ObjectStore {
        let region = env::var("AWS_DEFAULT_REGION").expect("AWS_DEFAULT_REGION is a required environment variable, see the README for further information");

        if region == "localstore" {
            let storage_directory = env::current_dir()
                .expect("Failed to get current working directory")
                .join("object_storage");
            create_dir_all(&storage_directory)
                .await
                .expect("Failed to create object storage directory");

            return ObjectStore::Local { storage_directory };
        }

        let endpoint_url =
            env::var("AWS_ENDPOINT_URL").expect("AWS_ENDPOINT_URL is missing, see README");
        let bucket_name =
            env::var("AWS_S3_BUCKET_NAME").expect("AWS_S3_BUCKET_NAME is missing, see README");
        let access_key_id =
            env::var("AWS_ACCESS_KEY_ID").expect("AWS_ACCESS_KEY_ID is missing, see README");
        let secret_access_key = env::var("AWS_SECRET_ACCESS_KEY")
            .expect("AWS_SECRET_ACCESS_KEY is missing, see README");

        let credentials = Credentials::new(access_key_id, secret_access_key, None, None, "custom");

        let cors_rule = CorsRule::builder()
            .allowed_origins("*")
            .allowed_methods("GET")
            .allowed_methods("PUT")
            .allowed_headers("*")
            .expose_headers("ETag")
            .expose_headers("Content-Length")
            .max_age_seconds(3600)
            .build()
            .expect("failed to build cors rule");

        let cors_config = CorsConfiguration::builder()
            .cors_rules(cors_rule)
            .build()
            .expect("failed to build cors configuration");

        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .endpoint_url(endpoint_url)
            .region(Region::new(region))
            .credentials_provider(credentials)
            .load()
            .await;
        let s3_client = S3Client::new(&config);

        s3_client
            .put_bucket_cors()
            .bucket(&bucket_name)
            .cors_configuration(cors_config)
            .send()
            .await
            .expect("failed to set the cors configuration, does the API key have permission to run PutBucketCors?");

        ObjectStore::S3 {
            s3_client,
            bucket_name,
        }
    }
}

pub struct FileState {
    pub id: FileId,
    pub journal_id: JournalId,
    #[expect(unused)]
    pub hash: [u8; 16],
    pub name: String,
}

impl FileState {
    pub fn key(&self) -> String {
        format!("{}/{}-{}", self.journal_id, self.id, self.name)
    }
}

impl JournalService {
    pub async fn upload_file(
        &self,
        file_id: FileId,
        journal_id: JournalId,
        file_hash: [u8; 16],
        file_name: String,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::UPLOAD_FILE)
            .await?;

        let event_id = append_creation_event_in_tx(
            &mut tx,
            JournalDomainEvent::FileUploaded {
                file_id,
                journal_id,
                hash: file_hash,
                file_name,
            },
            None,
            authority,
            timestamp,
            None,
        )
        .await?;

        tx.commit().await?;
        Ok(event_id)
    }

    pub async fn delete_file(
        &self,
        file_id: FileId,
        journal_id: JournalId,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::OWNER)
            .await?;

        let stream_handle =
            get_stream_handle(&mut tx, journal_id, *file_id, StreamType::File).await?;

        let event_id = append_modification_event_in_tx(
            &mut tx,
            stream_handle,
            JournalDomainEvent::FileDeleted {
                file_id,
                journal_id,
            },
            None,
            authority,
            timestamp,
            None,
        )
        .await?;

        tx.commit().await?;
        Ok(event_id)
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
            return Err(InvalidJournal(journal_id));
        }

        let files = sqlx::query!(
            r#"
            SELECT f.id as "id: FileId", f.journal_id as "journal_id: JournalId", f.hash as "hash: [u8; 16]", f.name, e.authority as "authority: Authority", e.timestamp as "timestamp: Timestamp", e.display_timestamp as "display_timestamp: Timestamp"
            FROM files f
            INNER JOIN journal_events e
                ON e.stream_id = f.id AND f.journal_id = $1 AND e.stream_type = $2 AND e.event_type = $3
            WHERE f.journal_id = $1
            ORDER BY COALESCE(e.display_timestamp, e.timestamp) DESC
            "#,
            journal_id as JournalId,
            StreamType::File as i16,
            EventType::FileUploaded as i16,
        )
            .fetch_all(self.pool())
            .await?.into_iter().map(|f| {
            let timestamp = f.display_timestamp.unwrap_or(f.timestamp);

            (FileState {
                id: f.id,
                journal_id,
                hash: f.hash,
                name: f.name,
            }, authority, timestamp)}).collect();

        Ok(files)
    }

    pub(crate) async fn get_file(
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
            FileState,
            r#"
            SELECT f.id as "id: FileId", f.journal_id as "journal_id: JournalId", f.hash as "hash: [u8; 16]", f.name
            FROM files f
            WHERE f.journal_id = $1 AND f.id = $2
            "#,
            journal_id as JournalId,
            file_id as FileId,
        )
            .fetch_optional(self.pool())
            .await?;

        if let Some(file) = file {
            return Ok(file);
        }

        Err(JournalError::InvalidFile(file_id))
    }
}
