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

id!(FundId, Ident::new16());

#[expect(unused)]
pub struct FundState {
    pub id: FundId,
    pub journal_id: JournalId,
    pub name: Name,
    pub balance: i64,
}

impl JournalService {
    pub async fn create_fund(
        &self,
        fund_id: FundId,
        journal_id: JournalId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::CREATE_FUND)
            .await?;

        let event_id = append_creation_event_in_tx(
            &mut tx,
            JournalDomainEvent::FundCreated {
                journal_id,
                fund_id,
                fund_name: name,
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

    pub async fn list_journal_funds(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(FundState, Authority, Timestamp)>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(InvalidJournal(journal_id));
        }

        let funds = sqlx::query!(
            r#"
            SELECT f.id as "id: FundId", f.journal_id as "journal_id: JournalId", f.name as "name: Name", f.balance, e.authority as "authority: Authority", e.timestamp as "timestamp: Timestamp", e.display_timestamp as "display_timestamp: Timestamp"
            FROM funds f
            INNER JOIN journal_events e
                ON f.journal_id = $1 AND e.stream_id = f.id AND e.stream_type = $2 AND e.event_type = $3
            WHERE e.journal_id = $1
            ORDER BY COALESCE(e.display_timestamp, e.timestamp) DESC
            "#,
            journal_id as JournalId,
            StreamType::Fund as i16,
            EventType::FundCreated as i16,
        )
            .fetch_all(self.pool())
            .await?.into_iter().map(|f| {
            let timestamp = f.display_timestamp.unwrap_or(f.timestamp);

            (FundState {
                id: f.id,
                journal_id,
                name: f.name,
                balance: f.balance,
            },
             authority, timestamp)
        }).collect();

        Ok(funds)
    }
}
