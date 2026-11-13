use super::JewelImportError;
use super::conversion::PreparedEvent;
use super::reconciliation::{ImportReport, PreparedImport};
use crate::authority::Authority;
use crate::journal::error::{JournalError, JournalResult};
use crate::journal::file::FileId;
use crate::journal::service::stream_lock::{append_creation_event_in_tx, assert_stream_exists};
use crate::journal::service::{JournalService, StreamType};
use crate::journal::{JournalId, Permissions};
use axum_login::tracing::error;
use serde::Serialize;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, watch};

pub type ImportJobs = Arc<Mutex<HashMap<JournalId, ImportJob>>>;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportPhase {
    Preparing,
    Review,
    Saving,
    Projecting,
    Complete,
    Failed,
}

#[derive(Clone, Serialize)]
pub struct ImportStatus {
    pub phase: ImportPhase,
    pub completed: i64,
    pub total: i64,
    pub message: String,
    pub report: Option<ImportReport>,
}

impl ImportStatus {
    pub fn preparing() -> Self {
        Self {
            phase: ImportPhase::Preparing,
            completed: 0,
            total: 0,
            message: "Reading the Jewel database and checking offerings.".into(),
            report: None,
        }
    }

    pub fn set_phase(&mut self, phase: ImportPhase, message: &str) {
        self.phase = phase;
        self.message = message.into();
        self.completed = 0;
        self.total = 0;
    }
}

pub struct ImportJob {
    pub file_id: FileId,
    pub status: watch::Sender<ImportStatus>,
    pub prepared: Option<PreparedImport>,
}

pub struct ImportReceipt {
    pub first_event_id: i64,
    pub last_event_id: i64,
    pub total_events: i64,
}

pub fn import_permissions() -> Permissions {
    Permissions::READ
        | Permissions::ADD_ACCOUNT
        | Permissions::CREATE_ACTIVITY
        | Permissions::CREATE_FUND
        | Permissions::APPEND_TRANSACTION
        | Permissions::ADD_DONOR
        | Permissions::ADD_DONATION
}

pub fn supervise(
    status: watch::Sender<ImportStatus>,
    task: impl Future<Output = JournalResult<()>> + Send + 'static,
) {
    tokio::spawn(async move {
        let failure = match tokio::spawn(task).await {
            Ok(Ok(())) => return,
            Ok(Err(import_error)) => {
                error!("Jewel import failed: {import_error:?}");
                match import_error {
                    JournalError::JewelImport(import_error) => import_error.to_string(),
                    _ => "The import could not finish. Please try again or contact support.".into(),
                }
            }
            Err(task_error) => {
                error!("Jewel import task failed: {task_error:?}");
                "The import was interrupted. Refresh this page to check its status.".into()
            }
        };
        status.send_modify(|status| status.set_phase(ImportPhase::Failed, &failure));
    });
}

impl JournalService {
    pub async fn jewel_import_receipt(
        &self,
        journal_id: JournalId,
        file_id: FileId,
    ) -> JournalResult<Option<ImportReceipt>> {
        Ok(sqlx::query_as!(
            ImportReceipt,
            "SELECT first_event_id, last_event_id, total_events FROM jewel_imports WHERE journal_id = $1 AND file_id = $2",
            journal_id as JournalId,
            file_id as FileId,
        ).fetch_optional(&self.pool).await?)
    }

    pub async fn import_jewel_database(
        &self,
        journal_id: JournalId,
        file_id: FileId,
        events: Vec<PreparedEvent>,
        authority: Authority,
        status: &watch::Sender<ImportStatus>,
    ) -> JournalResult<ImportReceipt> {
        let mut transaction = self.pool.begin().await?;
        self.check_permissions_in_tx(
            &mut transaction,
            journal_id,
            authority,
            import_permissions(),
        )
        .await?;
        assert_stream_exists(&mut transaction, journal_id, *file_id, StreamType::File).await?;
        let inserted = sqlx::query_scalar!(
            "INSERT INTO jewel_imports (journal_id, file_id) VALUES ($1, $2) ON CONFLICT DO NOTHING RETURNING file_id",
            journal_id as JournalId,
            file_id as FileId,
        ).fetch_optional(&mut *transaction).await?;
        if inserted.is_none() {
            return Err(JewelImportError::AlreadyImported.into());
        }
        let total_events = i64::try_from(events.len()).map_err(|_| {
            JewelImportError::InvalidData("This database contains too many records".into())
        })?;
        status.send_modify(|status| {
            status.set_phase(ImportPhase::Saving, "Saving the imported records.");
            status.total = total_events;
        });
        let mut first_event_id = 0;
        let mut last_event_id = 0;
        for (index, event) in events.into_iter().enumerate() {
            last_event_id = append_creation_event_in_tx(
                &mut transaction,
                event.payload,
                None,
                authority,
                event.timestamp,
                None,
            )
            .await?;
            if first_event_id == 0 {
                first_event_id = last_event_id;
            }
            let completed = index as i64 + 1;
            if completed % 100 == 0 || completed == total_events {
                status.send_modify(|status| status.completed = completed);
            }
        }
        sqlx::query!(
            "UPDATE jewel_imports SET first_event_id = $3, last_event_id = $4, total_events = $5 WHERE journal_id = $1 AND file_id = $2",
            journal_id as JournalId,
            file_id as FileId,
            first_event_id,
            last_event_id,
            total_events,
        ).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(ImportReceipt {
            first_event_id,
            last_event_id,
            total_events,
        })
    }

    pub async fn track_jewel_projection(
        &self,
        journal_id: JournalId,
        receipt: ImportReceipt,
        status: watch::Sender<ImportStatus>,
    ) -> JournalResult<()> {
        status.send_modify(|status| {
            status.set_phase(ImportPhase::Projecting, "Records saved. Updating balances and transaction lists; large databases can take a few minutes.");
            status.total = receipt.total_events;
        });
        loop {
            let checkpoint = sqlx::query_scalar!(
                "SELECT last_processed_event_id FROM journal_event_listener WHERE id = 'Journal Projection'"
            ).fetch_one(&self.pool).await?.unwrap_or(-1);
            if receipt.total_events == 0 || checkpoint >= receipt.last_event_id {
                status.send_modify(|status| {
                    status.set_phase(
                        ImportPhase::Complete,
                        "Import complete. Balances and transaction lists are up to date.",
                    );
                    status.total = receipt.total_events;
                    status.completed = receipt.total_events;
                });
                return Ok(());
            }
            let completed = sqlx::query_scalar!(
                r#"SELECT COUNT(*) AS "count!" FROM journal_events
                WHERE journal_id = $1 AND event_id >= $2 AND event_id <= $3 AND event_id <= $4"#,
                journal_id as JournalId,
                receipt.first_event_id,
                receipt.last_event_id,
                checkpoint,
            )
            .fetch_one(&self.pool)
            .await?;
            status.send_modify(|status| status.completed = completed.min(receipt.total_events));
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}
