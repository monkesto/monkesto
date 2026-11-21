use crate::journal::service::{EventId, JournalService};
use crate::journal::transaction::entry::EntryKind;
use crate::journal::{JournalDomainEvent, JournalId};
use crate::proto::journal::transaction::entry::entry::ProtoEntryKind;
use axum_login::tracing::error;
use prost::Message;
use serde_json::{Value, json};
use sqlx::postgres::PgListener;
use sqlx::types::Json;
use sqlx::{Acquire, PgPool};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::time::{sleep, timeout};

const BATCH_SIZE: i64 = 1000;
const POLL_INTERVAL: Duration = Duration::from_secs(5);
pub(super) const APPLICATION_CHANNEL: &str = "journal_projection";

pub(super) async fn install_sql(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;

    // Serialize startup installs before replacing the triggers used by event writers.
    sqlx::raw_sql("LOCK TABLE journal_events IN SHARE ROW EXCLUSIVE MODE")
        .execute(&mut *tx)
        .await?;
    for script in [
        include_str!("projection/sql/assign_journal_event_id.sql"),
        include_str!("projection/sql/signal_journal_events.sql"),
        include_str!("projection/sql/apply_journal_projection_batch.sql"),
    ] {
        sqlx::raw_sql(script).execute(&mut *tx).await?;
    }

    tx.commit().await
}

pub(super) async fn run(pool: PgPool) -> Result<!, sqlx::Error> {
    let mut listener = PgListener::connect_with(&pool).await?;
    listener.listen("journal_events").await?;

    loop {
        match project_incoming_events(&pool).await {
            Ok(true) => continue,
            Ok(false) => {}
            Err(err) => {
                error!(%err, "failed to claim or commit journal projection batch");
                sleep(POLL_INTERVAL).await;
                continue;
            }
        }

        tokio::select! {
            _ = sleep(POLL_INTERVAL) => {}
            notification = listener.recv() => {
                if let Err(err) = notification {
                    error!(%err, "failed to receive journal event notification");
                    sleep(POLL_INTERVAL).await;
                }
            }
        }
    }
}

async fn project_incoming_events(pool: &PgPool) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let Some(claim) = sqlx::query!(
        r#"SELECT p.journal_id AS "journal_id: JournalId", p.last_processed_event_id
         FROM journal_projection_checkpoint p
         WHERE p.retry_after <= now() AND EXISTS (
             SELECT 1 FROM journal_events e
             WHERE e.journal_id = p.journal_id AND e.event_id > p.last_processed_event_id
         )
         ORDER BY p.last_processed_event_id, p.journal_id
         LIMIT 1
         FOR UPDATE OF p SKIP LOCKED"#,
    )
    .fetch_optional(&mut *tx)
    .await?
    else {
        tx.rollback().await?;
        return Ok(false);
    };

    let journal_id = claim.journal_id;
    let checkpoint = claim.last_processed_event_id;

    // Keep the journal lock while recording a retry after a failed batch.
    let mut batch = tx.begin().await?;
    let applied = async {
        let events = sqlx::query!(
            r#"SELECT event_id, payload AS "payload: JournalDomainEvent" FROM journal_events
         WHERE journal_id = $1 AND event_id > $2
         ORDER BY event_id
         LIMIT $3"#,
            journal_id as JournalId,
            checkpoint,
            BATCH_SIZE,
        )
        .fetch_all(&mut *batch)
        .await?;

        if events.is_empty() {
            return Ok(());
        }

        let mut records = Vec::with_capacity(events.len());
        for row in events {
            if row.payload.journal_id() != journal_id {
                return Err(sqlx::Error::Protocol(format!(
                    "journal {journal_id} event {} has a different journal in its payload",
                    row.event_id,
                )));
            }
            records.push(projection_record(row.event_id, row.payload));
        }

        sqlx::query!(
            "WITH advanced AS (
                 UPDATE journal_projection_checkpoint
                 SET last_processed_event_id = apply_journal_projection_batch($1, $2),
                     retry_after = '-infinity'
                 WHERE journal_id = $1
                 RETURNING journal_id
             )
             SELECT pg_notify($3, journal_id) FROM advanced",
            journal_id as JournalId,
            Json(records) as Json<Vec<Value>>,
            APPLICATION_CHANNEL,
        )
        .execute(&mut *batch)
        .await?;
        Ok(())
    }
    .await;

    match applied {
        Ok(()) => batch.commit().await?,
        Err(err) => {
            batch.rollback().await?;
            error!(%err, %journal_id, checkpoint, "failed to apply journal projection batch");
            sqlx::query!(
                "UPDATE journal_projection_checkpoint
                 SET retry_after = now() + interval '5 seconds'
                 WHERE journal_id = $1",
                journal_id as JournalId,
            )
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(true)
}

pub(super) async fn forward_applications(
    mut listener: PgListener,
    applications: broadcast::Sender<JournalId>,
) {
    loop {
        match listener.recv().await {
            Ok(notification) => {
                if let Ok(journal_id) = notification.payload().parse() {
                    let _ = applications.send(journal_id);
                }
            }
            Err(sqlx::Error::PoolClosed) => return,
            Err(err) => {
                error!(%err, "failed to receive journal projection notification");
                sleep(POLL_INTERVAL).await;
            }
        }
    }
}

impl JournalService {
    pub async fn wait_for(&self, event_id: EventId) {
        if event_id == 0 {
            return;
        }

        // Subscribe before checking the checkpoint so a concurrent commit can wake this waiter.
        let mut applications = self.projection_applications.subscribe();
        loop {
            let progress = sqlx::query!(
                r#"SELECT e.journal_id AS "journal_id: JournalId",
                          p.last_processed_event_id >= e.event_id AS "projected!"
                   FROM journal_events e
                   JOIN journal_projection_checkpoint p USING (journal_id)
                   WHERE e.event_id = $1"#,
                event_id,
            )
            .fetch_optional(&self.pool)
            .await;
            let journal_id = match progress {
                Ok(Some(progress)) if progress.projected => return,
                Ok(Some(progress)) => Some(progress.journal_id),
                Ok(None) => None,
                Err(err) => {
                    error!(%err, event_id, "failed to read journal projection checkpoint");
                    None
                }
            };

            let _ = timeout(Duration::from_secs(1), async {
                loop {
                    match applications.recv().await {
                        Ok(applied) if journal_id.is_none_or(|id| id == applied) => break,
                        Ok(_) => {}
                        Err(_) => break,
                    }
                }
            })
            .await;
        }
    }
}

fn projection_record(event_id: EventId, event: JournalDomainEvent) -> Value {
    use JournalDomainEvent::*;

    match event {
        JournalCreated { owner, name, .. } => json!({
            "event_id": event_id, "kind": "JournalCreated", "owner_id": owner.as_str(),
            "name": name.as_ref()
        }),
        JournalDeleted { .. } => json!({ "event_id": event_id, "kind": "JournalDeleted" }),
        MemberAdded {
            user_id,
            permissions,
            ..
        } => json!({
            "event_id": event_id, "kind": "MemberAdded", "user_id": user_id.as_str(),
            "permissions": permissions.bits()
        }),
        MemberPermissionsUpdated {
            user_id,
            permissions,
            ..
        } => json!({
            "event_id": event_id, "kind": "MemberPermissionsUpdated", "user_id": user_id.as_str(),
            "permissions": permissions.bits()
        }),
        MemberRemoved { user_id, .. } => json!({
            "event_id": event_id, "kind": "MemberRemoved", "user_id": user_id.as_str()
        }),
        AccountCreated {
            account_id,
            name,
            account_type,
            ..
        } => json!({
            "event_id": event_id, "kind": "AccountCreated", "id": account_id.as_str(),
            "name": name.as_ref(), "account_type": account_type as i16
        }),
        AccountRenamed {
            account_id,
            new_name,
            ..
        } => json!({
            "event_id": event_id, "kind": "AccountRenamed", "id": account_id.as_str(),
            "name": new_name.as_ref()
        }),
        AccountDeleted { account_id, .. } => json!({
            "event_id": event_id, "kind": "AccountDeleted", "id": account_id.as_str()
        }),
        TransactionCreated {
            transaction_id,
            financial_period,
            memo,
            ..
        } => json!({
            "event_id": event_id, "kind": "TransactionCreated", "id": transaction_id.as_str(),
            "financial_period": financial_period as i16, "memo": memo.as_ref().map(AsRef::<str>::as_ref)
        }),
        FileUploaded {
            file_id,
            hash,
            file_name,
            ..
        } => json!({
            "event_id": event_id, "kind": "FileUploaded", "id": file_id.as_str(),
            "hash": hex::encode(hash), "name": file_name
        }),
        FileDeleted { file_id, .. } => json!({
            "event_id": event_id, "kind": "FileDeleted", "id": file_id.as_str()
        }),
        FundCreated {
            fund_id, fund_name, ..
        } => json!({
            "event_id": event_id, "kind": "FundCreated", "id": fund_id.as_str(),
            "name": fund_name.as_ref()
        }),
        EntryCreated {
            entry_id,
            transaction_id,
            amount,
            entry_side,
            entry_kind,
            ..
        } => {
            let (account, activity, fund) = match &entry_kind {
                EntryKind::Account { account_id } => (Some(account_id.as_str()), None, None),
                EntryKind::Activity {
                    activity_id,
                    fund_id,
                    ..
                } => (None, Some(activity_id.as_str()), Some(fund_id.as_str())),
            };
            json!({
                "event_id": event_id, "kind": "EntryCreated", "id": entry_id.as_str(),
                "transaction_id": transaction_id.as_str(), "amount": amount, "entry_side": entry_side as i16,
                "entry_kind": hex::encode(ProtoEntryKind::from(entry_kind).encode_to_vec()),
                "account_id": account, "activity_id": activity, "fund_id": fund
            })
        }
        ActivityCreated {
            activity_id,
            activity_name,
            activity_kind,
            ..
        } => json!({
            "event_id": event_id, "kind": "ActivityCreated", "id": activity_id.as_str(),
            "name": activity_name.as_ref(), "activity_kind": activity_kind as i16
        }),
        DonorCreated { donor_id, name, .. } => json!({
            "event_id": event_id, "kind": "DonorCreated", "id": donor_id.as_str(),
            "name": name.as_ref()
        }),
        DonationEntryCreated {
            donation_entry_id,
            fund_id,
            donor_id,
            donation_id,
            amount,
            instrument,
            check_num,
            ..
        } => json!({
            "event_id": event_id, "kind": "DonationEntryCreated", "id": donation_entry_id.as_str(),
            "fund_id": fund_id.as_str(), "donor_id": donor_id.as_str(), "donation_id": donation_id.as_str(),
            "amount": amount, "instrument": instrument as i16, "check_num": check_num.map(|check| check.to_string())
        }),
        DonationCreated {
            donation_id,
            deposit_account_id,
            donation_total,
            ..
        } => json!({
            "event_id": event_id, "kind": "DonationCreated", "id": donation_id.as_str(),
            "account_id": deposit_account_id.as_str(), "amount": donation_total
        }),
    }
}

#[cfg(test)]
mod tests;
