use crate::authority::Authority;
use crate::id;
use crate::id::Ident;
use crate::journal::domain::{ActivityEvent, JournalDomainEvent};
use crate::journal::member::JournalMember;
use crate::journal::{Journal, JournalError, JournalId, Permissions, validate_permissions};
use crate::name::Name;
use crate::status::Status;
use crate::time_provider::Timestamp;
use axum_test::expect_json::__private::serde_trampoline::{Deserialize, Serialize};
use disintegrate::{Decision, StateMutate, StateQuery};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use thiserror::Error;

id!(ActivityId, Ident::new16());

#[derive(Debug, Default, Copy, Clone, PartialEq, Serialize, Deserialize, Eq)]
#[repr(i8)]
pub enum ActivityType {
    #[default]
    Income,
    Expense,
    Transfer,
}

#[derive(Debug, Error, PartialEq)]
#[error("{0}")]
pub struct ActivityTypeFromIntError(pub i8);

impl TryFrom<i8> for ActivityType {
    type Error = ActivityTypeFromIntError;

    fn try_from(value: i8) -> Result<Self, Self::Error> {
        match value {
            x if x == ActivityType::Income as i8 => Ok(ActivityType::Income),
            x if x == ActivityType::Expense as i8 => Ok(ActivityType::Expense),
            x if x == ActivityType::Transfer as i8 => Ok(ActivityType::Transfer),
            _ => Err(ActivityTypeFromIntError(value)),
        }
    }
}

impl Type<Postgres> for ActivityType {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&i16 as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for ActivityType {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <i16 as Encode<Postgres>>::encode(*self as i16, buf)
    }
}

impl<'r> Decode<'r, Postgres> for ActivityType {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let int16 = <i16 as Decode<Postgres>>::decode(value)?;
        Ok(Self::try_from(int16 as i8)?)
    }
}

#[derive(Debug, Default, Clone, StateQuery, Serialize, Deserialize)]
#[state_query(ActivityEvent)]
pub struct Activity {
    pub activity_id: ActivityId,
    pub journal_id: JournalId,
    pub activity_type: ActivityType,
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
                activity_type,
                ..
            } => {
                self.activity_id = activity_id;
                self.journal_id = journal_id;
                self.name = activity_name;
                self.activity_type = activity_type;
                self.status = Status::Valid;
            }
        }
    }
}

pub struct CreateActivity {
    activity_id: ActivityId,
    journal_id: JournalId,
    activity_name: Name,
    activity_type: ActivityType,
    authority: Authority,
    timestamp: Timestamp,
}

impl CreateActivity {
    pub fn new(
        activity_id: ActivityId,
        journal_id: JournalId,
        activity_name: Name,
        activity_type: ActivityType,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            activity_id,
            journal_id,
            activity_name,
            activity_type,
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
            activity_type: self.activity_type,
            authority: self.authority,
            timestamp: self.timestamp,
        }])
    }
}
