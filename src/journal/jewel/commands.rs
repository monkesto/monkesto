use super::JewelImportError;
use super::jobs::{ImportJob, ImportPhase, ImportStatus, import_permissions, supervise};
use super::reconciliation::{ImportResolutions, PreparedImport};
use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::journal::JournalId;
use crate::journal::error::{JournalError, JournalResult};
use crate::journal::file::FileId;
use crate::journal::service::JournalService;
use crate::{BackendType, StateType};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum_login::AuthSession;
use std::str::FromStr;
use tokio::sync::watch;

pub async fn authorize_import(
    service: &JournalService,
    journal_id: JournalId,
    file_id: FileId,
    authority: Authority,
) -> JournalResult<()> {
    let mut transaction = service.pool.begin().await?;
    service
        .check_permissions_in_tx(
            &mut transaction,
            journal_id,
            authority,
            import_permissions(),
        )
        .await?;
    transaction.commit().await?;
    service.get_file(file_id, journal_id, authority).await?;
    Ok(())
}

fn status_response(status: ImportStatus) -> Response {
    ([(header::CACHE_CONTROL, "no-store")], Json(status)).into_response()
}

pub async fn start_import(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((journal_id, file_id)): Path<(String, String)>,
) -> Result<Response, JournalError> {
    let user = get_user(session)?;
    let authority = Authority::Direct(Actor::User(user.id));
    let journal_id = JournalId::from_str(&journal_id)?;
    let file_id = FileId::from_str(&file_id)?;
    let service = state.journal_service;
    authorize_import(&service, journal_id, file_id, authority).await?;
    let receipt = service.jewel_import_receipt(journal_id, file_id).await?;
    let mut jobs = service.jewel_imports.lock().await;
    if let Some(job) = jobs.get(&journal_id) {
        let status = job.status.borrow().clone();
        if job.file_id == file_id && status.phase != ImportPhase::Failed {
            return Ok(status_response(status));
        }
        if job.file_id != file_id
            && !matches!(
                status.phase,
                ImportPhase::Complete | ImportPhase::Failed | ImportPhase::Review
            )
        {
            return Err(JewelImportError::InvalidData(
                "Another Jewel import is running for this journal. Please wait for it to finish."
                    .into(),
            )
            .into());
        }
    }
    let (status, _) = watch::channel(ImportStatus::preparing());
    jobs.insert(
        journal_id,
        ImportJob {
            file_id,
            status: status.clone(),
            prepared: None,
        },
    );
    drop(jobs);
    let initial_status = status.borrow().clone();
    if let Some(receipt) = receipt {
        supervise(status.clone(), async move {
            service
                .track_jewel_projection(journal_id, receipt, status)
                .await
        });
    } else {
        supervise(status.clone(), async move {
            let data = service
                .get_jewel_db(journal_id, file_id, authority, 0, 0)
                .await?;
            let prepared = tokio::task::spawn_blocking(move || PreparedImport::new(data))
                .await
                .map_err(|_| {
                    JewelImportError::InvalidData("Could not analyze the Jewel database".into())
                })??;
            let report = prepared.report.clone();
            let mut jobs = service.jewel_imports.lock().await;
            let job = jobs
                .get_mut(&journal_id)
                .filter(|job| job.file_id == file_id)
                .ok_or_else(|| {
                    JewelImportError::InvalidData("This import has been replaced".into())
                })?;
            job.prepared = Some(prepared);
            status.send_modify(|status| {
                status.set_phase(
                    ImportPhase::Review,
                    "Review the import and resolve any discrepancies before saving.",
                );
                status.report = Some(report);
            });
            Ok(())
        });
    }
    Ok(status_response(initial_status))
}

pub async fn import_status(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((journal_id, file_id)): Path<(String, String)>,
) -> Result<Response, JournalError> {
    let user = get_user(session)?;
    let authority = Authority::Direct(Actor::User(user.id));
    let journal_id = JournalId::from_str(&journal_id)?;
    let file_id = FileId::from_str(&file_id)?;
    authorize_import(&state.journal_service, journal_id, file_id, authority).await?;
    let jobs = state.journal_service.jewel_imports.lock().await;
    let job = jobs
        .get(&journal_id)
        .filter(|job| job.file_id == file_id)
        .ok_or_else(|| {
            JewelImportError::InvalidData(
                "Import status is unavailable. Reload the import page to resume.".into(),
            )
        })?;
    Ok(status_response(job.status.borrow().clone()))
}

pub async fn confirm_import(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((journal_id, file_id)): Path<(String, String)>,
    Json(resolutions): Json<ImportResolutions>,
) -> Result<Response, JournalError> {
    let user = get_user(session)?;
    let authority = Authority::Direct(Actor::User(user.id));
    let journal_id = JournalId::from_str(&journal_id)?;
    let file_id = FileId::from_str(&file_id)?;
    let service = state.journal_service;
    authorize_import(&service, journal_id, file_id, authority).await?;
    let mut jobs = service.jewel_imports.lock().await;
    let job = jobs
        .get_mut(&journal_id)
        .filter(|job| job.file_id == file_id)
        .ok_or_else(|| {
            JewelImportError::InvalidData("Reload the import page before continuing".into())
        })?;
    let current = job.status.borrow().clone();
    if matches!(
        current.phase,
        ImportPhase::Saving | ImportPhase::Projecting | ImportPhase::Complete
    ) {
        return Ok(status_response(current));
    }
    let prepared = job.prepared.as_ref().ok_or_else(|| {
        JewelImportError::InvalidData(
            "Wait until the database has been checked before importing".into(),
        )
    })?;
    prepared.validate_resolutions(&resolutions)?;
    let prepared = job
        .prepared
        .take()
        .ok_or_else(|| JewelImportError::InvalidData("This import has already started".into()))?;
    let status = job.status.clone();
    status.send_modify(|status| {
        status.set_phase(ImportPhase::Saving, "Preparing the records to save.")
    });
    let initial_status = status.borrow().clone();
    drop(jobs);
    supervise(status.clone(), async move {
        let events = tokio::task::spawn_blocking(move || prepared.events(journal_id, &resolutions))
            .await
            .map_err(|_| {
                JewelImportError::InvalidData("Could not prepare the import records".into())
            })??;
        let receipt = service
            .import_jewel_database(journal_id, file_id, events, authority, &status)
            .await?;
        service
            .track_jewel_projection(journal_id, receipt, status)
            .await
    });
    Ok(status_response(initial_status))
}
