use crate::authority::Authority;
use crate::event_id::GetEventId;
use crate::id;
use crate::id::Ident;
use crate::journal::domain::{ActivityEvent, JournalDomainEvent};
use crate::journal::member::JournalMember;
use crate::journal::{
    Journal, JournalError, JournalId, JournalResult, JournalService, Permissions,
    validate_permissions,
};
use crate::name::Name;
use crate::status::Status;
use crate::time_provider::Timestamp;
use axum_test::expect_json::__private::serde_trampoline::{Deserialize, Serialize};
use disintegrate::{Decision, DecisionError, StateMutate, StateQuery};
use disintegrate_postgres::PgEventId;
use prost::Message;
use proto::event::journal::ProtoJournalDomainEvent;
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, FromRow, Postgres, Type};
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

#[expect(unused)]
pub struct ActivityState {
    pub id: ActivityId,
    pub journal_id: JournalId,
    pub name: Name,
    pub activity_type: ActivityType,
    pub balance: i64,
}

#[derive(FromRow)]
pub struct ActivityStateWithPayload {
    id: ActivityId,
    #[expect(unused)]
    journal_id: JournalId,
    name: Name,
    activity_type: ActivityType,
    balance: i64,
    payload: Vec<u8>,
}

impl JournalService {
    #[expect(unused)]
    pub async fn create_activity(
        &self,
        activity_id: ActivityId,
        journal_id: JournalId,
        name: Name,
        activity_type: ActivityType,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(CreateActivity::new(
                activity_id,
                journal_id,
                name,
                activity_type,
                authority,
                timestamp,
            ))
            .await?
            .event_id())
    }

    #[expect(unused)]
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
            return Err(JournalError::InvalidJournal(journal_id));
        }

        let activities = sqlx::query_as!(
            ActivityStateWithPayload,
            r#"
            SELECT a.id as "id: ActivityId", a.journal_id as "journal_id: JournalId", a.name as "name: Name", a.activity_type as "activity_type: ActivityType", a.balance, e.payload as "payload!"
            FROM activities a
            INNER JOIN event e
                ON e.account_id = a.id AND e.event_type = 'ActivityCreated'
            WHERE e.journal_id = $1
            "#,
            journal_id as JournalId)
            .fetch_all(&self.projection_pool)
            .await?;

        let mut activities_with_meta = Vec::with_capacity(activities.len());

        for activity in activities {
            let payload = JournalDomainEvent::try_from(ProtoJournalDomainEvent::decode(
                activity.payload.as_slice(),
            )?)?;

            match payload {
                JournalDomainEvent::ActivityCreated {
                    authority,
                    timestamp,
                    ..
                } => {
                    activities_with_meta.push((
                        ActivityState {
                            id: activity.id,
                            journal_id,
                            name: activity.name,
                            activity_type: activity.activity_type,
                            balance: activity.balance,
                        },
                        authority,
                        timestamp,
                    ));
                }
                _ => unreachable!("ActivityCreated events are filtered by the sql query"),
            }
        }

        Ok(activities_with_meta)
    }
}
