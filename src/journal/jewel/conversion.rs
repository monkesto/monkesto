use super::JewelImportError;
use super::accounting::{AccountKind, JournalKind};
use super::reconciliation::{ImportResolutions, PreparedImport, ReconciliationChoice, cents};
use crate::journal::JournalId;
use crate::journal::account::{AccountId, AccountType};
use crate::journal::activity::scope::ActivityScope;
use crate::journal::activity::{ActivityId, ActivityKind};
use crate::journal::donation::donation_entry::{DonationEntryId, DonationInstrument};
use crate::journal::donation::donor::DonorId;
use crate::journal::donation::{CheckNum, DonationId};
use crate::journal::error::JournalResult;
use crate::journal::event::JournalDomainEvent;
use crate::journal::fund::FundId;
use crate::journal::transaction::entry::{EntryId, EntryKind, EntrySide};
use crate::journal::transaction::memo::Memo;
use crate::journal::transaction::{FinancialPeriod, TransactionId};
use crate::name::Name;
use crate::time::{TimeProvider, Timestamp};
use chrono::Utc;
use std::collections::{BTreeMap, HashMap};

pub struct PreparedEvent {
    pub payload: JournalDomainEvent,
    pub timestamp: Timestamp,
}

struct ImportedFund {
    fund_id: FundId,
    activity_id: ActivityId,
}

struct EventBuilder {
    journal_id: JournalId,
    events: Vec<PreparedEvent>,
    accounts: HashMap<i64, AccountId>,
    funds: HashMap<i64, ImportedFund>,
    donors: HashMap<i64, DonorId>,
}

impl EventBuilder {
    fn push(&mut self, payload: JournalDomainEvent, timestamp: Timestamp) {
        self.events.push(PreparedEvent { payload, timestamp });
    }

    fn transaction(
        &mut self,
        balances: &BTreeMap<i64, i64>,
        memo: String,
        timestamp: Timestamp,
        financial_period: FinancialPeriod,
        scope: ActivityScope,
    ) -> JournalResult<()> {
        if balances.values().all(|amount| *amount == 0) {
            return Ok(());
        }
        let transaction_id = TransactionId::new();
        let mut entries = Vec::new();
        let mut difference = 0_i128;
        for (&account, &amount) in balances {
            if amount == 0 {
                continue;
            }
            let magnitude = amount.checked_abs().ok_or_else(|| {
                JewelImportError::InvalidData("Transaction amount is too large to import".into())
            })? as u64;
            // Source bank increases are debits; source fund increases are credits.
            // Income/expense metadata and journal type never flip these sides.
            let (entry_kind, entry_side) = if let Some(&account_id) = self.accounts.get(&account) {
                (
                    EntryKind::Account { account_id },
                    if amount < 0 {
                        EntrySide::Debit
                    } else {
                        EntrySide::Credit
                    },
                )
            } else {
                let fund = self
                    .funds
                    .get(&account)
                    .ok_or(JewelImportError::MissingAccount(account))?;
                (
                    EntryKind::Activity {
                        activity_id: fund.activity_id,
                        fund_id: fund.fund_id,
                        scope,
                    },
                    if amount < 0 {
                        EntrySide::Credit
                    } else {
                        EntrySide::Debit
                    },
                )
            };
            difference += match entry_side {
                EntrySide::Debit => i128::from(magnitude),
                EntrySide::Credit => -i128::from(magnitude),
            };
            entries.push(JournalDomainEvent::EntryCreated {
                entry_id: EntryId::new(),
                journal_id: self.journal_id,
                transaction_id,
                amount: magnitude,
                entry_side,
                entry_kind,
            });
        }
        if difference != 0 {
            return Err(JewelImportError::InvalidData(format!(
                "Unbalanced Jewel transaction on {} ({memo}): debits minus credits = {difference} cents",
                timestamp.format("%Y-%m-%d")
            )).into());
        }
        for entry in entries {
            self.push(entry, timestamp);
        }
        let mut memo = memo;
        if memo.len() > 100 {
            let mut end = 100;
            while !memo.is_char_boundary(end) {
                end -= 1;
            }
            memo.truncate(end);
        }
        self.push(
            JournalDomainEvent::TransactionCreated {
                transaction_id,
                journal_id: self.journal_id,
                financial_period,
                memo: Some(Memo::try_new(memo)?),
            },
            timestamp,
        );
        Ok(())
    }
}

impl PreparedImport {
    pub fn events(
        &self,
        journal_id: JournalId,
        resolutions: &ImportResolutions,
    ) -> JournalResult<Vec<PreparedEvent>> {
        self.validate_resolutions(resolutions)?;
        let mut builder = EventBuilder {
            journal_id,
            events: Vec::new(),
            accounts: HashMap::new(),
            funds: HashMap::new(),
            donors: HashMap::new(),
        };
        let mut timestamp = Utc::now();
        for (&account, source) in &self.data.accounts {
            if !source.active
                || AccountKind::try_from(source.account_type)? == AccountKind::BeginningBalance
            {
                continue;
            }
            timestamp += std::time::Duration::from_millis(1);
            let name = Name::try_new(source.name.clone())?;
            if AccountKind::try_from(source.account_type)? == AccountKind::Bank {
                let account_id = AccountId::new();
                builder.accounts.insert(account, account_id);
                builder.push(
                    JournalDomainEvent::AccountCreated {
                        journal_id,
                        account_id,
                        account_type: AccountType::Asset,
                        name,
                    },
                    timestamp.get_time(),
                );
            } else {
                let activity_id = ActivityId::new();
                let fund_id = FundId::new();
                builder.funds.insert(
                    account,
                    ImportedFund {
                        fund_id,
                        activity_id,
                    },
                );
                builder.push(
                    JournalDomainEvent::ActivityCreated {
                        activity_id,
                        journal_id,
                        activity_name: name.clone(),
                        // Preserve existing activity metadata. Entry sides depend
                        // only on the bank/fund role and signed source amount.
                        activity_kind: if source.account_type < 3 {
                            ActivityKind::Income
                        } else {
                            ActivityKind::Expense
                        },
                    },
                    timestamp.get_time(),
                );
                timestamp += std::time::Duration::from_millis(1);
                builder.push(
                    JournalDomainEvent::FundCreated {
                        fund_id,
                        journal_id,
                        fund_name: name,
                    },
                    timestamp.get_time(),
                );
            }
        }
        for (&name_id, source) in &self.data.names {
            if !source.donor {
                continue;
            }
            timestamp += std::time::Duration::from_millis(1);
            let donor_id = DonorId::new();
            builder.donors.insert(name_id, donor_id);
            builder.push(
                JournalDomainEvent::DonorCreated {
                    donor_id,
                    journal_id,
                    name: Name::try_new(source.name.clone())?,
                },
                timestamp.get_time(),
            );
        }

        for (date, opening) in &self.opening_batches {
            let period = FinancialPeriod::from(date.and_time(chrono::NaiveTime::MIN).and_utc());
            builder.transaction(
                &opening.balances,
                format!(
                    "{date} opening balances (Jewel journals {}–{})",
                    opening.first_journal_id, opening.last_journal_id
                ),
                opening.timestamp,
                period,
                ActivityScope::Internal,
            )?;
        }

        for (&journal_entry_id, journal) in &self.data.journals {
            let journal_kind = JournalKind::try_from(journal.journal_type_id)?;
            if journal_kind == JournalKind::OpeningBalances {
                continue;
            }
            let timestamp = journal.date.get_time();
            let period = FinancialPeriod::from(journal.accounting_date);
            let offering_id = self.data.offering_journals.get(&journal_entry_id).copied();
            if offering_id.is_some_and(|offering_id| self.ignored_offerings.contains(&offering_id))
            {
                continue;
            }
            let choice = offering_id
                .and_then(|offering_id| resolutions.offerings.get(&offering_id))
                .copied();
            let conversion = offering_id.and_then(|offering_id| self.conversions.get(&offering_id));
            let scope = match journal_kind {
                JournalKind::Deposit | JournalKind::Payment => ActivityScope::External,
                JournalKind::OpeningBalances | JournalKind::Transfer => ActivityScope::Internal,
            };
            if choice == Some(ReconciliationChoice::JournalOnly)
                || conversion.is_some_and(|conversion| {
                    conversion.use_journal && conversion.positive_total == 0
                })
                || conversion.is_none_or(|conversion| !conversion.has_contributions)
            {
                let mut balances = BTreeMap::new();
                for entry in self
                    .data
                    .journal_entries
                    .get(&journal_entry_id)
                    .ok_or(JewelImportError::MissingJournalEntries(journal_entry_id))?
                {
                    super::reconciliation::add_balance(
                        &mut balances,
                        entry.account_id,
                        cents(entry.amount)?,
                    )?;
                }
                builder.transaction(&balances, journal.memo.clone(), timestamp, period, scope)?;
                continue;
            }

            let offering_id =
                offering_id.ok_or(JewelImportError::MissingOffering(journal_entry_id))?;
            let offering = self
                .data
                .offerings
                .get(&offering_id)
                .ok_or(JewelImportError::MissingOffering(offering_id))?;
            let conversion = self
                .conversions
                .get(&offering_id)
                .ok_or(JewelImportError::MissingOffering(offering_id))?;
            if conversion.positive_total > 0 {
                let donation_id = DonationId::new();
                let deposit_account_id = *builder
                    .accounts
                    .get(&conversion.bank_account_id)
                    .ok_or(JewelImportError::MissingAccount(conversion.bank_account_id))?;
                for envelope in self.data.envelopes.get(&offering_id).into_iter().flatten() {
                    let cash = cents(envelope.cash_total)?;
                    let check = cents(envelope.check_total)?;
                    let instrument = if offering.offering_source == 5 {
                        DonationInstrument::Online
                    } else if check == 0 {
                        DonationInstrument::Cash
                    } else if cash == 0 {
                        DonationInstrument::Check
                    } else {
                        DonationInstrument::Mixed
                    };
                    for contribution in conversion
                        .contributions
                        .get(&envelope.envelope_id)
                        .into_iter()
                        .flatten()
                    {
                        let amount = contribution.amount;
                        if amount <= 0 {
                            continue;
                        }
                        let donor_id = *builder
                            .donors
                            .get(&envelope.name_id)
                            .ok_or(JewelImportError::MissingDonor(envelope.name_id))?;
                        let fund_id = builder
                            .funds
                            .get(&contribution.account_id)
                            .ok_or(JewelImportError::MissingAccount(contribution.account_id))?
                            .fund_id;
                        builder.push(
                            JournalDomainEvent::DonationEntryCreated {
                                donation_entry_id: DonationEntryId::new(),
                                journal_id,
                                fund_id,
                                donor_id,
                                donation_id,
                                amount: amount as u64,
                                instrument,
                                check_num: envelope
                                    .check_num
                                    .clone()
                                    .map(CheckNum::try_new)
                                    .transpose()?,
                            },
                            offering.date.get_time(),
                        );
                    }
                }
                builder.push(
                    JournalDomainEvent::DonationCreated {
                        donation_id,
                        journal_id,
                        deposit_account_id,
                        donation_total: conversion.positive_total as u64,
                    },
                    timestamp,
                );
            }

            if conversion.use_journal {
                // Retain new donor entries, then use the source journal for the
                // remaining withdrawal and fund movements, including any fees.
                let imported = conversion.balances_before_reversals()?;
                let remaining = conversion.journal_difference(&imported)?;
                builder.transaction(&remaining, journal.memo.clone(), timestamp, period, scope)?;
            } else {
                for reversal in &conversion.reversals {
                    let mut balances: BTreeMap<_, _> = reversal
                        .funds
                        .iter()
                        .map(|(&fund, &amount)| (fund, -amount))
                        .collect();
                    super::reconciliation::add_balance(
                        &mut balances,
                        conversion.bank_account_id,
                        -reversal.total,
                    )?;
                    builder.transaction(
                        &balances,
                        format!(
                            "Jewel offering {offering_id}: reversal envelope {}",
                            reversal.envelope_id
                        ),
                        timestamp,
                        period,
                        scope,
                    )?;
                }
            }

            for correction in &conversion.fund_reclassifications {
                let donor = self
                    .data
                    .names
                    .get(&correction.name_id)
                    .ok_or(JewelImportError::MissingDonor(correction.name_id))?;
                builder.transaction(
                    &correction.funds,
                    format!(
                        "{} offering correction — {}",
                        offering.date.format("%Y-%m-%d"),
                        donor.name
                    ),
                    timestamp,
                    period,
                    ActivityScope::Internal,
                )?;
            }

            if choice == Some(ReconciliationChoice::Adjustment) {
                let imported = conversion.balances()?;
                let adjustment = conversion.journal_difference(&imported)?;
                builder.transaction(
                    &adjustment,
                    format!("Jewel offering {offering_id}: reconciliation adjustment"),
                    timestamp,
                    period,
                    scope,
                )?;
            }
        }
        Ok(builder.events)
    }
}
