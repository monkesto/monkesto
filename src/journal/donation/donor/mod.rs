pub mod commands;
pub mod views;

use crate::authority::Authority;
use crate::id;
use crate::id::Ident;
use crate::journal::error::JournalError::InvalidJournal;
use crate::journal::error::JournalResult;
use crate::journal::event::JournalDomainEvent;
use crate::journal::service::stream_lock::append_creation_event_in_tx;
use crate::journal::service::{EventId, EventType, JournalService, StreamType};
use crate::journal::{JournalId, Permissions};
use crate::name::Name;
use crate::time::Timestamp;

id!(DonorId, Ident::new16());

pub struct DonorState {
    pub id: DonorId,
    #[expect(unused)]
    pub journal_id: JournalId,
    pub name: Name,
    pub total_donated: u64,
}

impl JournalService {
    pub async fn create_donor(
        &self,
        donor_id: DonorId,
        journal_id: JournalId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::ADD_DONOR)
            .await?;

        let event_id = append_creation_event_in_tx(
            &mut tx,
            JournalDomainEvent::DonorCreated {
                journal_id,
                donor_id,
                name,
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

    pub async fn list_journal_donors(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(DonorState, Authority, Timestamp)>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(InvalidJournal(journal_id));
        }

        let donors = sqlx::query!(
            r#"
            SELECT d.id as "id: DonorId", d.journal_id as "journal_id: JournalId", d.name as "name: Name", d.total_donated, e.authority as "authority: Authority", e.timestamp as "timestamp: Timestamp", e.display_timestamp as "display_timestamp: Timestamp"
            FROM donors d
            INNER JOIN journal_events e
                ON e.journal_id = $1 AND e.stream_id = d.id AND e.stream_type = $2 AND e.event_type = $3
            WHERE d.journal_id = $1
            ORDER BY COALESCE(e.display_timestamp, e.timestamp) DESC
            "#,
            journal_id as JournalId,
            StreamType::Donor as i16,
            EventType::DonorCreated as i16,
        )
            .fetch_all(self.pool())
            .await?.into_iter().map(|d| {
            let timestamp = d.display_timestamp.unwrap_or(d.timestamp);
            (DonorState {
                id: d.id,
                journal_id,
                total_donated: d.total_donated.unsigned_abs(),
                name: d.name,
            }, d.authority, timestamp)
        }).collect();

        Ok(donors)
    }
}
