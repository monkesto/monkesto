use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::journal::file::{FileId, S3_CLIENT};
use crate::journal::layout::layout;
use crate::journal::{JournalError, JournalId, JournalResult, JournalService};
use crate::{BackendType, StateType};
use axum::extract::{Path, State};
use axum::response::Redirect;
use axum_login::AuthSession;
use maud::{Markup, html};
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{ConnectOptions, FromRow};
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
#[derive(FromRow, Debug)]
#[expect(unused)]
pub struct JewelAccount {
    account_id: i64,
    /// mystery int
    account_type: i64,
    name: String,
    parent_id: Option<i64>,
    tax_deductible: bool,
    // allow_posting?
    local_income: bool,
    local_expense: bool,
    permanent: bool,
    active: bool,
}

pub struct JewelData {
    accounts: Vec<JewelAccount>,
}

impl JournalService {
    #[allow(clippy::disallowed_methods)]
    pub async fn get_jewel_db(
        &self,
        journal_id: JournalId,
        file_id: FileId,
        authority: Authority,
    ) -> JournalResult<JewelData> {
        let file_key = self.get_file(file_id, journal_id, authority).await?.key();

        if let Some(s3_client) = S3_CLIENT.clone() {
            let mut response = s3_client
                .get_object()
                .bucket("monkesto")
                .key(file_key)
                .send()
                .await?;
            let mut mdb_file =
                NamedTempFile::new().map_err(|e| JewelImportError::Io(e.to_string()))?;
            let sqlite_file =
                NamedTempFile::new().map_err(|e| JewelImportError::Io(e.to_string()))?;

            let mdb_file_path = mdb_file.path().to_string_lossy().to_string();
            let sqlite_file_path = sqlite_file.path().to_string_lossy();

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

            let schema = Command::new("mdb-schema")
                .args([mdb_file_path.as_ref(), "sqlite"])
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
                .args(["-1", mdb_file_path.as_ref()])
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
                    .args(["-I", "sqlite", "-q", "'", mdb_file_path.as_ref(), table])
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

            #[allow(clippy::type_complexity)]
            let raw_accounts: Vec<(i64, i64, String, Option<i64>, bool, bool, bool, bool, bool)> = sqlx::query_as(
                r#"
                    select AccountID, AccountType, Name, ParentAccountID, TaxDeductible, LocalIncome, LocalExpense, Permanent, Active FROM Accounts
                    "#
            ).fetch_all(&mut conn).await?;

            let mut accounts = Vec::with_capacity(raw_accounts.len());

            for (
                account_id,
                account_type,
                name,
                parent_id,
                tax_deductible,
                local_income,
                local_expense,
                permanent,
                active,
            ) in raw_accounts
            {
                accounts.push(JewelAccount {
                    account_id,
                    account_type,
                    name,
                    parent_id,
                    tax_deductible,
                    local_income,
                    local_expense,
                    permanent,
                    active,
                })
            }

            Ok(JewelData { accounts })
        } else {
            Err(JournalError::InvalidS3Credentials)
        }
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
                    @for account in data.accounts {
                        p {
                            (format!("{:?}", account))
                        }

                        br;
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
