use crate::authority::Authority;
use crate::event_id::GetEventId;
use crate::id;
use crate::id::Ident;
use crate::journal::domain::{FundEvent, JournalDomainEvent};
use crate::journal::member::JournalMember;
use crate::journal::{
    Journal, JournalError, JournalId, JournalResult, JournalService, Permissions,
    validate_permissions,
};
use crate::name::Name;
use crate::status::Status;
use crate::time_provider::Timestamp;
use disintegrate::{Decision, DecisionError, StateMutate, StateQuery};
use disintegrate_postgres::PgEventId;
use prost::Message;
use proto::event::journal::ProtoJournalDomainEvent;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

id!(FundId, Ident::new16());

#[derive(Debug, Default, Clone, StateQuery, Serialize, Deserialize)]
#[state_query(FundEvent)]
pub struct Fund {
    pub fund_id: FundId,
    pub journal_id: JournalId,
    pub name: Name,
    pub status: Status,
}

impl Fund {
    fn new(journal_id: JournalId, fund_id: FundId) -> Self {
        Self {
            fund_id,
            journal_id,
            ..Default::default()
        }
    }
}

impl StateMutate for Fund {
    fn mutate(&mut self, event: Self::Event) {
        match event {
            FundEvent::FundCreated {
                fund_id,
                journal_id,
                fund_name,
                ..
            } => {
                self.fund_id = fund_id;
                self.journal_id = journal_id;
                self.name = fund_name;
                self.status = Status::Valid;
            }
        }
    }
}

pub struct CreateFund {
    fund_id: FundId,
    journal_id: JournalId,
    fund_name: Name,
    authority: Authority,
    timestamp: Timestamp,
}

impl CreateFund {
    pub fn new(
        fund_id: FundId,
        journal_id: JournalId,
        fund_name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            fund_id,
            journal_id,
            fund_name,
            authority,
            timestamp,
        }
    }
}

impl Decision for CreateFund {
    type Event = JournalDomainEvent;
    type StateQuery = (Journal, JournalMember, Fund);
    type Error = JournalError;

    fn state_query(&self) -> Self::StateQuery {
        (
            Journal::new(self.journal_id),
            JournalMember::new(
                self.journal_id,
                self.authority.user_id().unwrap_or_default(),
            ),
            Fund::new(self.journal_id, self.fund_id),
        )
    }

    fn process(
        &self,
        (journal, actor, fund): &Self::StateQuery,
    ) -> Result<Vec<Self::Event>, Self::Error> {
        if fund.status.found() {
            return Err(JournalError::FundIdCollision(self.fund_id));
        }

        if !journal.status.valid() {
            return Err(JournalError::InvalidJournal(self.journal_id));
        }

        if !validate_permissions(
            actor,
            self.authority,
            journal.owner,
            Permissions::CREATE_FUND,
        ) {
            return Err(JournalError::Permissions(Permissions::CREATE_FUND));
        }

        Ok(vec![JournalDomainEvent::FundCreated {
            fund_id: self.fund_id,
            journal_id: self.journal_id,
            fund_name: self.fund_name.clone(),
            authority: self.authority,
            timestamp: self.timestamp,
        }])
    }
}

#[expect(unused)]
pub struct FundState {
    pub id: FundId,
    pub journal_id: JournalId,
    pub name: Name,
    pub balance: i64,
}

#[derive(FromRow)]
pub struct FundStateWithPayload {
    id: FundId,
    #[expect(unused)]
    journal_id: JournalId,
    name: Name,
    balance: i64,
    payload: Vec<u8>,
}

impl JournalService {
    #[expect(unused)]
    pub async fn create_fund(
        &self,
        fund_id: FundId,
        journal_id: JournalId,
        name: Name,
        authority: Authority,
        timestamp: Timestamp,
    ) -> Result<PgEventId, DecisionError<JournalError>> {
        Ok(self
            .decision_maker
            .make(CreateFund::new(
                fund_id, journal_id, name, authority, timestamp,
            ))
            .await?
            .event_id())
    }

    #[expect(unused)]
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
            return Err(JournalError::InvalidJournal(journal_id));
        }

        let funds = sqlx::query_as!(
            FundStateWithPayload,
            r#"
            SELECT f.id as "id: FundId", f.journal_id as "journal_id: JournalId", f.name as "name: Name", f.balance, e.payload as "payload!"
            FROM funds f
            INNER JOIN event e
                ON e.account_id = f.id AND e.event_type = 'FundCreated'
            WHERE e.journal_id = $1
            "#,
            journal_id as JournalId)
            .fetch_all(&self.projection_pool)
            .await?;

        let mut funds_with_meta = Vec::with_capacity(funds.len());

        for fund in funds {
            let payload = JournalDomainEvent::try_from(ProtoJournalDomainEvent::decode(
                fund.payload.as_slice(),
            )?)?;

            match payload {
                JournalDomainEvent::FundCreated {
                    authority,
                    timestamp,
                    ..
                } => {
                    funds_with_meta.push((
                        FundState {
                            id: fund.id,
                            journal_id,
                            name: fund.name,
                            balance: fund.balance,
                        },
                        authority,
                        timestamp,
                    ));
                }
                _ => unreachable!("FundCreated events are filtered by the sql query"),
            }
        }

        Ok(funds_with_meta)
    }
}
