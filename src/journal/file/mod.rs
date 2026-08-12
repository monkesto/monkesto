use crate::authority::Authority;
use crate::id;
use crate::id::Ident;
use crate::journal::domain::{FileEvent, JournalDomainEvent, JournalEvent};
use crate::journal::{Journal, JournalError, JournalId};
use crate::status::Status;
use crate::time_provider::Timestamp;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::config::Credentials;
use aws_types::region::Region;
use disintegrate::{Decision, StateMutate, StateQuery};
use serde::{Deserialize, Serialize};
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
