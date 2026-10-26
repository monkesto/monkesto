pub mod commands;
pub mod kind;
pub mod scope;
pub mod views;

use crate::authority::Authority;
use crate::id;
use crate::id::Ident;
pub(crate) use crate::journal::activity::kind::ActivityKind;
use crate::journal::error::JournalError::InvalidJournal;
use crate::journal::error::JournalResult;
use crate::journal::event::JournalDomainEvent;
use crate::journal::service::stream_lock::append_creation_event_in_tx;
use crate::journal::service::{EventId, EventType, JournalService, StreamType};
use crate::journal::{JournalId, Permissions};
use crate::name::Name;
use crate::time::Timestamp;

id!(ActivityId, Ident::new16());

#[expect(unused)]
pub struct ActivityState {
    pub id: ActivityId,
    pub journal_id: JournalId,
    pub name: Name,
    pub activity_type: ActivityKind,
    pub balance: i64,
}

impl JournalService {
    pub async fn create_activity(
        &self,
        activity_id: ActivityId,
        journal_id: JournalId,
        name: Name,
        activity_kind: ActivityKind,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::CREATE_ACTIVITY)
            .await?;

        let event_id = append_creation_event_in_tx(
            &mut tx,
            JournalDomainEvent::ActivityCreated {
                journal_id,
                activity_id,
                activity_name: name,
                activity_kind,
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

    pub async fn list_journal_activities(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(ActivityState, Authority, Timestamp)>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(InvalidJournal(journal_id));
        }

        let activities = sqlx::query!(
            r#"
            SELECT a.id as "id: ActivityId", a.journal_id as "journal_id: JournalId", a.name as "name: Name", a.activity_kind as "activity_kind: ActivityKind", a.balance, e.authority as "authority: Authority", e.timestamp as "timestamp: Timestamp", e.display_timestamp as "display_timestamp: Timestamp"
            FROM activities a
            INNER JOIN journal_events e
                ON a.journal_id = $1 AND e.stream_id = a.id AND e.stream_type = $2 AND e.event_type = $3
            WHERE e.journal_id = $1
            ORDER BY COALESCE(e.display_timestamp, e.timestamp) DESC
            "#,
            journal_id as JournalId,
            StreamType::Activity as i16,
            EventType::ActivityCreated as i16,
        )
            .fetch_all(self.pool())
            .await?.into_iter().map(|a| {
            let timestamp = a.display_timestamp.unwrap_or(a.timestamp);
            (ActivityState {
                id: a.id,
                journal_id,
                name: a.name,
                activity_type: a.activity_kind,
                balance: 0,
            }, a.authority, timestamp)
        }).collect();

        Ok(activities)
    }
}
