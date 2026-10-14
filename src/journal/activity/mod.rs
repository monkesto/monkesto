pub mod kind;
pub mod scope;

use crate::authority::Authority;
use crate::id;
use crate::id::Ident;
pub(crate) use crate::journal::activity::kind::ActivityKind;
use crate::journal::error::JournalError;
use crate::journal::event::{ActivityEvent, JournalDomainEvent};
use crate::journal::member::JournalMember;
use crate::journal::{Journal, JournalId, Permissions, validate_permissions};
use crate::name::Name;
use crate::status::Status;
use crate::time::Timestamp;
use disintegrate::{Decision, StateMutate, StateQuery};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

id!(ActivityId, Ident::new16());

#[derive(Debug, Default, Clone, StateQuery, Serialize, Deserialize)]
#[state_query(ActivityEvent)]
pub struct Activity {
    pub activity_id: ActivityId,
    pub journal_id: JournalId,
    pub activity_kind: ActivityKind,
    pub name: Name,
    pub status: Status,
}

impl Activity {
    fn new(journal_id: JournalId, activity_id: ActivityId) -> Self {
        Self {
            activity_id,
            journal_id,
            ..Default::default()
        }
    }
}

impl StateMutate for Activity {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            ActivityEvent::ActivityCreated {
                activity_id,
                journal_id,
                activity_name,
                activity_kind,
                ..
            } => {
                self.activity_id = activity_id;
                self.journal_id = journal_id;
                self.name = activity_name;
                self.activity_kind = activity_kind;
                self.status = Status::Valid;
            }
        }
    }
}

pub struct CreateActivity {
    activity_id: ActivityId,
    journal_id: JournalId,
    activity_name: Name,
    activity_kind: ActivityKind,
    authority: Authority,
    timestamp: Timestamp,
}

impl CreateActivity {
    pub fn new(
        activity_id: ActivityId,
        journal_id: JournalId,
        activity_name: Name,
        activity_kind: ActivityKind,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            activity_id,
            journal_id,
            activity_name,
            activity_kind,
            authority,
            timestamp,
        }
    }
}

impl Decision for CreateActivity {
    type Event = JournalDomainEvent;
    type StateQuery = (Journal, JournalMember, Activity);
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        (
            Journal::new(self.journal_id),
            JournalMember::new(
                self.journal_id,
                self.authority.user_id().unwrap_or_default(),
            ),
            Activity::new(self.journal_id, self.activity_id),
        )
    }

    fn process(
        &self,
        (journal, actor, fund): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if fund.status.found() {
            return Err(JournalError::ActivityIdCollision(self.activity_id));
        }

        if !journal.status.valid() {
            return Err(JournalError::InvalidJournal(self.journal_id));
        }

        if !validate_permissions(
            actor,
            self.authority,
            journal.owner,
            Permissions::CREATE_ACTIVITY,
        ) {
            return Err(JournalError::Permissions(Permissions::CREATE_ACTIVITY));
        }

        Ok(vec![JournalDomainEvent::ActivityCreated {
            activity_id: self.activity_id,
            journal_id: self.journal_id,
            activity_name: self.activity_name.clone(),
            activity_kind: self.activity_kind,
            authority: self.authority,
            timestamp: self.timestamp,
        }])
    }
}

#[expect(unused)]
pub struct ActivityState {
    pub id: ActivityId,
    pub journal_id: JournalId,
    pub name: Name,
    pub activity_type: ActivityKind,
    pub balance: i64,
}

#[derive(FromRow)]
pub struct ActivityStateWithPayload {
    pub id: ActivityId,
    #[expect(unused)]
    pub journal_id: JournalId,
    pub name: Name,
    pub activity_kind: ActivityKind,
    pub balance: i64,
    pub payload: JournalDomainEvent,
}
