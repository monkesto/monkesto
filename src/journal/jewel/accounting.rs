//! Jewel stores bank/fund balance changes, not uniformly signed debits/credits.
use super::JewelImportError;
use super::extract::JewelData;
use super::reconciliation::{add_balance, cents};
use crate::time::{TimeProvider, Timestamp};
use chrono::NaiveDate;
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AccountKind {
    BeginningBalance,
    Bank,
    ConferenceFund,
    LocalFund,
}

impl TryFrom<i64> for AccountKind {
    type Error = JewelImportError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::BeginningBalance),
            1 => Ok(Self::Bank),
            2 => Ok(Self::ConferenceFund),
            3 => Ok(Self::LocalFund),
            _ => Err(JewelImportError::InvalidData(format!(
                "Unsupported Jewel AccountType {value}"
            ))),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum JournalKind {
    OpeningBalances,
    Deposit,
    Payment,
    Transfer,
}

impl TryFrom<i64> for JournalKind {
    type Error = JewelImportError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::OpeningBalances),
            2 => Ok(Self::Deposit),
            4 => Ok(Self::Payment),
            5 => Ok(Self::Transfer),
            _ => Err(JewelImportError::InvalidData(format!(
                "Unsupported Jewel JournalTypeID {value}"
            ))),
        }
    }
}

pub struct OpeningBatch {
    pub balances: BTreeMap<i64, i64>,
    pub timestamp: Timestamp,
    pub first_journal_id: i64,
    pub last_journal_id: i64,
    pub journal_count: usize,
}

/// Jewel splits opening bank balances and fund allocations across separate records.
/// A date identifies a candidate batch; equal bank/fund totals are required to import it.
pub fn opening_batches(
    data: &JewelData,
) -> Result<BTreeMap<NaiveDate, OpeningBatch>, JewelImportError> {
    let mut batches: BTreeMap<NaiveDate, OpeningBatch> = BTreeMap::new();
    for (&journal_id, journal) in &data.journals {
        if JournalKind::try_from(journal.journal_type_id)? != JournalKind::OpeningBalances {
            continue;
        }
        let date = journal.accounting_date.date_naive();
        let batch = batches.entry(date).or_insert_with(|| OpeningBatch {
            balances: BTreeMap::new(),
            timestamp: journal.date.get_time(),
            first_journal_id: journal_id,
            last_journal_id: journal_id,
            journal_count: 0,
        });
        batch.journal_count += 1;
        batch.last_journal_id = journal_id;
        if journal.date < batch.timestamp.0 {
            batch.timestamp = journal.date.get_time();
        }
        for entry in data
            .journal_entries
            .get(&journal_id)
            .ok_or(JewelImportError::MissingJournalEntries(journal_id))?
        {
            add_balance(&mut batch.balances, entry.account_id, cents(entry.amount)?)?;
        }
    }
    for (date, batch) in &batches {
        let mut difference = 0_i128;
        for (&account_id, &amount) in &batch.balances {
            if amount == 0 {
                continue;
            }
            let source = data
                .accounts
                .get(&account_id)
                .ok_or(JewelImportError::MissingAccount(account_id))?;
            match AccountKind::try_from(source.account_type)? {
                AccountKind::Bank => difference += i128::from(amount),
                AccountKind::ConferenceFund | AccountKind::LocalFund => {
                    difference -= i128::from(amount);
                }
                AccountKind::BeginningBalance => {
                    return Err(JewelImportError::InvalidData(format!(
                        "Beginning Balance account {account_id} has a nonzero posting in opening balances on {date}"
                    )));
                }
            }
        }
        if difference != 0 {
            return Err(JewelImportError::InvalidData(format!(
                "Opening balances on {date} (Jewel journals {}–{}) do not balance: bank and fund totals differ by {difference} cents",
                batch.first_journal_id, batch.last_journal_id
            )));
        }
    }
    Ok(batches)
}
