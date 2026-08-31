use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::journal::file::{FileId, ObjectStore};
use crate::journal::layout::layout;
use crate::journal::{JournalId, JournalResult, JournalService};
use crate::{BackendType, StateType};
use axum::extract::{Path, State};
use axum::response::Redirect;
use axum_login::AuthSession;
use jewel_extractor::{JewelData, jewel_extract};
use maud::{Markup, html};
use sqlx::ConnectOptions;
use sqlx::sqlite::SqliteConnectOptions;
use std::fs::File;
use std::io;
use std::io::Write;
use std::process::Stdio;
use std::str::FromStr;
use tempfile::NamedTempFile;
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Error, Debug, PartialEq)]
pub enum JewelImportError {
    #[error("failed to create a temp file or read bytes from a stream")]
    Io(String),
    #[error("failed to start a child process")]
    StartChildProcess(String),
    #[error("failed to write to a file or stdin of a child process")]
    Write(String),
    #[error("child process exited with an error: {0}")]
    ChildProcessExitFailure(String),
    #[error("the database uses an outdated version of jewel: {0}; jewel 9.0 or newer is required")]
    OutdatedJewelVersion(f64),
}

impl JournalService {
    pub async fn get_jewel_db(
        &self,
        journal_id: JournalId,
        file_id: FileId,
        authority: Authority,
    ) -> JournalResult<JewelData> {
        let file_key = self.get_file(file_id, journal_id, authority).await?.key();

        let (mdb_file, mdb_file_path) = match &self.object_store {
            ObjectStore::S3 {
                s3_client,
                bucket_name,
            } => {
                let mut response = s3_client
                    .get_object()
                    .bucket(bucket_name)
                    .key(file_key)
                    .send()
                    .await?;

                let mut mdb_file =
                    NamedTempFile::new().map_err(|e| JewelImportError::Io(e.to_string()))?;

                while let Some(bytes) = response
                    .body
                    .try_next()
                    .await
                    .map_err(|e| JewelImportError::Io(e.to_string()))?
                {
                    mdb_file
                        .write_all(bytes.as_ref())
                        .map_err(|e| JewelImportError::Write(e.to_string()))?;
                }

                mdb_file
                    .flush()
                    .map_err(|e| JewelImportError::Write(e.to_string()))?;

                let file_path = mdb_file.path().to_string_lossy().to_string();

                (mdb_file, file_path)
            }
            ObjectStore::Local { storage_directory } => {
                let stored_file_path = storage_directory.join(file_key);
                let mut stored_file = File::open(&stored_file_path)
                    .map_err(|e| JewelImportError::Io(e.to_string()))?;
                let mut mdb_file =
                    NamedTempFile::new().map_err(|e| JewelImportError::Io(e.to_string()))?;

                io::copy(&mut stored_file, &mut mdb_file)
                    .map_err(|e| JewelImportError::Io(e.to_string()))?;

                let file_path = mdb_file.path().to_string_lossy().to_string();

                (mdb_file, file_path)
            }
        };

        let sqlite_file = NamedTempFile::new().map_err(|e| JewelImportError::Io(e.to_string()))?;

        let sqlite_file_path = sqlite_file.path().to_string_lossy();

        let schema = Command::new("mdb-schema")
            .arg(&mdb_file_path)
            .arg("sqlite")
            .output()
            .await
            .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

        if !schema.status.success() {
            Err(JewelImportError::ChildProcessExitFailure(
                String::from_utf8_lossy(schema.stderr.as_slice()).to_string(),
            ))?;
        }

        let mut sqlite_process = Command::new("sqlite3")
            .arg(sqlite_file_path.as_ref())
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

        if let Some(mut stdin) = sqlite_process.stdin.take() {
            stdin
                .write_all(&schema.stdout)
                .await
                .map_err(|e| JewelImportError::Write(e.to_string()))?;
        }

        sqlite_process
            .wait()
            .await
            .map_err(|e| JewelImportError::Io(e.to_string()))?;

        let tables = Command::new("mdb-tables")
            .arg("-1")
            .arg(&mdb_file_path)
            .output()
            .await
            .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

        if !tables.status.success() {
            Err(JewelImportError::ChildProcessExitFailure(
                String::from_utf8_lossy(tables.stderr.as_slice()).to_string(),
            ))?;
        }

        let tables_str = String::from_utf8_lossy(&tables.stdout);

        for table in tables_str.lines() {
            let export_output = Command::new("mdb-export")
                .arg("-I")
                .arg("sqlite")
                .arg("-q")
                .arg("'")
                .arg(&mdb_file_path)
                .arg(table)
                .output()
                .await
                .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

            if !export_output.status.success() {
                Err(JewelImportError::ChildProcessExitFailure(
                    String::from_utf8_lossy(export_output.stderr.as_slice()).to_string(),
                ))?;
            }

            let mut insert_cmd = Command::new("sqlite3")
                .arg(sqlite_file_path.as_ref())
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

            if let Some(mut stdin) = insert_cmd.stdin.take() {
                stdin
                    .write_all(&export_output.stdout)
                    .await
                    .map_err(|e| JewelImportError::Write(e.to_string()))?;
            }

            insert_cmd
                .wait()
                .await
                .map_err(|e| JewelImportError::Io(e.to_string()))?;
        }

        // we no longer need the mdb file
        drop(mdb_file);

        let mut conn = SqliteConnectOptions::new()
            .filename(sqlite_file_path.as_ref())
            .connect()
            .await?;

        // old versions of jewel stored the database version as an int
        #[allow(clippy::disallowed_methods)]
        let version: f64 = sqlx::query_scalar(
            r#"
                SELECT CAST(DBVersion AS REAL) as DBVersion FROM GeneralInfo;
            "#,
        )
        .fetch_one(&mut conn)
        .await?;

        if version < 9.0 {
            Err(JewelImportError::OutdatedJewelVersion(version))?;
        }

        Ok(jewel_extract(&mut conn).await?)
    }
}

// NOTE: Production implementations should call get_jewel_db as a secondary web request because it has to download a file from the internet and transform it with cli tools
// a latency of 500ms or more is expected from this endpoint
pub async fn view_db(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((journal_id, file_id)): Path<(String, String)>,
) -> Result<Markup, Redirect> {
    let user = get_user(session)?;
    let user_authority = Authority::Direct(Actor::User(user.id));
    let journal_id_res = JournalId::from_str(&journal_id);
    let file_id_res = FileId::from_str(&file_id);

    let markup = if let Ok(journal_id) = journal_id_res
        && let Ok(file_id) = file_id_res
    {
        html! {
            @match state.journal_service.get_jewel_db(journal_id, file_id, user_authority).await {
                Ok(data) => ul {
                    h1 {
                        "accounts"
                    }

                    @for account in data.accounts {
                        p {
                            (format!("{:?}", account))
                        }

                        br;
                    }

                    h1 {
                        "names"
                    }

                    @for name in data.names {
                        p {
                            (format!("{:?}", name))
                        }
                    }
                },
                Err(e) => p {(e.to_string())}
            }
        }
    } else {
        html! {
            p {
                "invalid journal or file id"
            }
        }
    };

    Ok(layout(None, true, Some(&journal_id), markup))
}
