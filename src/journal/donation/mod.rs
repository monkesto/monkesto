use crate::authority::Authority;
use crate::id;
use crate::id::Ident;
use crate::journal::account::AccountId;
use crate::journal::donation::donation_entry::{DonationEntryId, DonationInstrument};
use crate::journal::donation::donor::DonorId;
use crate::journal::error::JournalError::InvalidJournal;
use crate::journal::error::JournalResult;
use crate::journal::event::JournalDomainEvent;
use crate::journal::fund::FundId;
use crate::journal::service::stream_lock::append_creation_event_in_tx;
use crate::journal::service::{EventId, EventType, JournalService, StreamType};
use crate::journal::{JournalId, Permissions};
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use sqlx::encode::IsNull;
use sqlx::error::BoxDynError;
use sqlx::{Database, Decode, Encode, Postgres, Type};
use std::fmt::{Display, Formatter};
use thiserror::Error;

pub mod donation_entry;
pub mod donor;
pub mod views;

id!(DonationId, Ident::new16());

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckNum(String);

#[derive(Debug, Error, PartialEq)]
#[error("Check numbers must be a maximum of 50 characters in length")]
pub struct CheckNumFromStrError(String);

impl CheckNum {
    // for now, allow any kind of character in the check-num field to match jewel
    pub fn try_new(s: String) -> Result<CheckNum, CheckNumFromStrError> {
        if s.len() > 50 {
            return Err(CheckNumFromStrError(s));
        }

        Ok(CheckNum(s))
    }
}

impl Display for CheckNum {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Type<Postgres> for CheckNum {
    fn type_info() -> <Postgres as Database>::TypeInfo {
        <&str as Type<Postgres>>::type_info()
    }
}

impl<'q> Encode<'q, Postgres> for CheckNum {
    fn encode_by_ref(
        &self,
        buf: &mut <Postgres as Database>::ArgumentBuffer<'q>,
    ) -> Result<IsNull, BoxDynError> {
        <&str as Encode<Postgres>>::encode(self.to_string().as_str(), buf)
    }
}

impl<'r> Decode<'r, Postgres> for CheckNum {
    fn decode(value: <Postgres as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let str = <String as Decode<Postgres>>::decode(value)?;
        Ok(CheckNum::try_new(str)?)
    }
}

pub struct DonationState {
    pub id: DonationId,
    #[expect(unused)]
    pub journal_id: JournalId,
    pub entries: Vec<DonationEntry>,
    pub deposit_account: AccountId,
    pub total: u64,
}

#[expect(unused)]
pub struct DonationEntry {
    id: DonationEntryId,
    fund_id: FundId,
    donor_id: DonorId,
    amount: u64,
    instrument: DonationInstrument,
    check_num: Option<CheckNum>,
}

// type needed for postges to deserialize embedded entries
type DonationEntryTuple = (
    DonationEntryId,
    FundId,
    DonorId,
    i64,
    DonationInstrument,
    Option<CheckNum>,
);

impl JournalService {
    #[expect(unused)]
    pub async fn create_donation(
        &self,
        donation_id: DonationId,
        journal_id: JournalId,
        deposit_account_id: AccountId,
        entries: Vec<DonationEntry>,
        authority: Authority,
        timestamp: Timestamp,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;

        self.check_permissions_in_tx(&mut tx, journal_id, authority, Permissions::ADD_DONATION)
            .await?;

        let mut donation_total = 0;

        for entry in entries {
            donation_total += entry.amount;

            append_creation_event_in_tx(
                &mut tx,
                JournalDomainEvent::DonationEntryCreated {
                    donation_entry_id: entry.id,
                    journal_id,
                    fund_id: entry.fund_id,
                    donor_id: entry.donor_id,
                    donation_id,
                    amount: entry.amount,
                    instrument: entry.instrument,
                    check_num: entry.check_num,
                },
                None,
                authority,
                timestamp,
                None,
            )
            .await?;
        }

        let event_id = append_creation_event_in_tx(
            &mut tx,
            JournalDomainEvent::DonationCreated {
                donation_id,
                journal_id,
                deposit_account_id,
                donation_total,
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

    pub async fn list_journal_donations(
        &self,
        journal_id: JournalId,
        authority: Authority,
    ) -> JournalResult<Vec<(DonationState, Authority, Timestamp)>> {
        if !self
            .get_effective_permissions(journal_id, authority)
            .await?
            .contains(Permissions::READ)
        {
            return Err(InvalidJournal(journal_id));
        }

        let donations = sqlx::query!(
            r#"
            SELECT d.id as "id: DonationId", d.journal_id as "journal_id: JournalId", d.deposit_account_id as "deposit_account_id: AccountId", d.donation_total, e.authority as "authority: Authority", e.timestamp as "timestamp: Timestamp", e.display_timestamp as "display_timestamp: Timestamp",
                ARRAY(
                    SELECT (en.id, en.fund_id , en.donor_id, en.amount, en.instrument, en.check_num)
                    FROM donation_entries en
                    WHERE en.donation_id = d.id AND en.journal_id = d.journal_id
                ) AS "entries!: Vec<DonationEntryTuple>"
            FROM donations d
            INNER JOIN journal_events e
                ON d.journal_id = $1 AND e.stream_id = d.id AND e.stream_type = $2 AND e.event_type = $3
            WHERE d.journal_id = $1
            ORDER BY COALESCE(e.display_timestamp, e.timestamp) DESC
            "#,
            journal_id as JournalId,
            StreamType::Donation as i16,
            EventType::DonationCreated as i16,
        )
            .fetch_all(self.pool())
            .await?
            .into_iter()
            .map(|d| {
            let timestamp = d.display_timestamp.unwrap_or(d.timestamp);
            (DonationState {
                id: d.id,
                journal_id,
                entries: d.entries.into_iter().map(|(id, fund_id, donor_id, amount, instrument, check_num)| DonationEntry {
                    id,
                    fund_id,
                    donor_id,
                    amount: amount.unsigned_abs(),
                    instrument,
                    check_num,
                }).collect(),
                deposit_account: d.deposit_account_id,
                total: d.donation_total as u64,
            }, d.authority, timestamp)
        }).collect();

        Ok(donations)
    }
}
