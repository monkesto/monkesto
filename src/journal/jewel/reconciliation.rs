use super::JewelImportError;
use super::accounting::{AccountKind, JournalKind, OpeningBatch, opening_batches};
use super::extract::JewelData;
use crate::journal::error::JournalResult;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Serialize)]
pub struct AccountDifference {
    pub account_id: i64,
    pub name: String,
    pub contributions: i64,
    pub journal: i64,
}

#[derive(Clone, Serialize)]
pub struct ReconciliationIssue {
    pub offering_id: i64,
    pub date: String,
    pub deposit_date: String,
    pub offering_total: i64,
    pub envelope_total: i64,
    pub positive_contributions: i64,
    pub contribution_total: i64,
    pub bank_total: i64,
    pub has_contributions: bool,
    pub differences: Vec<AccountDifference>,
    pub problems: Vec<String>,
}

#[derive(Clone, Serialize)]
pub struct ImportReport {
    pub accounts: usize,
    pub donors: usize,
    pub journals: usize,
    pub offerings: usize,
    pub reversals: usize,
    pub fund_reclassifications: usize,
    pub issues: Vec<ReconciliationIssue>,
}

#[derive(Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationChoice {
    JournalOnly,
    Contributions,
    Adjustment,
}

#[derive(Default, Deserialize)]
pub struct ImportResolutions {
    pub offerings: HashMap<i64, ReconciliationChoice>,
}

pub struct Reversal {
    pub envelope_id: i64,
    pub funds: BTreeMap<i64, i64>,
    pub total: i64,
}

pub struct PreparedContribution {
    pub account_id: i64,
    pub amount: i64,
    source_amount: i64,
}

pub struct FundReclassification {
    pub name_id: i64,
    pub funds: BTreeMap<i64, i64>,
}

pub struct OfferingConversion {
    pub use_journal: bool,
    pub bank_account_id: i64,
    pub positive_total: i64,
    pub funds: BTreeMap<i64, i64>,
    pub contributions: BTreeMap<i64, Vec<PreparedContribution>>,
    pub reversals: Vec<Reversal>,
    pub fund_reclassifications: Vec<FundReclassification>,
    pub journal: BTreeMap<i64, i64>,
    pub has_contributions: bool,
}

pub struct PreparedImport {
    pub data: JewelData,
    pub opening_batches: BTreeMap<chrono::NaiveDate, OpeningBatch>,
    pub report: ImportReport,
    pub conversions: BTreeMap<i64, OfferingConversion>,
    pub ignored_offerings: BTreeSet<i64>,
}

pub fn cents(amount: f64) -> Result<i64, JewelImportError> {
    let rounded = (amount * 100.0).round();
    if !rounded.is_finite() || rounded <= i64::MIN as f64 || rounded >= i64::MAX as f64 {
        return Err(JewelImportError::InvalidData(
            "Invalid monetary amount".into(),
        ));
    }
    Ok(rounded as i64)
}

pub fn add_balance(
    balances: &mut BTreeMap<i64, i64>,
    account_id: i64,
    amount: i64,
) -> Result<(), JewelImportError> {
    let balance = balances.entry(account_id).or_default();
    *balance = balance.checked_add(amount).ok_or_else(|| {
        JewelImportError::InvalidData("The total amount is too large to import".into())
    })?;
    Ok(())
}

fn contribution_fund_mapping(
    data: &JewelData,
    offering_id: i64,
    journal: &BTreeMap<i64, i64>,
) -> Result<BTreeMap<i64, i64>, JewelImportError> {
    let mut contributions = BTreeMap::new();
    for envelope in data.envelopes.get(&offering_id).into_iter().flatten() {
        for contribution in data
            .contributions
            .get(&envelope.envelope_id)
            .into_iter()
            .flatten()
        {
            add_balance(
                &mut contributions,
                contribution.account_id,
                cents(contribution.amount)?,
            )?;
        }
    }

    let mut sources = BTreeMap::<i64, Vec<i64>>::new();
    let mut destinations = BTreeMap::<i64, Vec<i64>>::new();
    for (&account_id, &amount) in &contributions {
        if amount != 0
            && !data.bank_accounts.contains(&account_id)
            && journal.get(&account_id).copied().unwrap_or_default() == 0
        {
            sources.entry(amount).or_default().push(account_id);
        }
    }
    for (&account_id, &amount) in journal {
        if amount != 0
            && !data.bank_accounts.contains(&account_id)
            && contributions.get(&account_id).copied().unwrap_or_default() == 0
        {
            destinations.entry(amount).or_default().push(account_id);
        }
    }

    // Match whole fund amounts, with exactly one candidate on each side.
    // Repeated amounts leave the offering available for explicit reconciliation.
    let mut mapping = BTreeMap::new();
    for (amount, source_accounts) in sources {
        if let [source] = source_accounts.as_slice()
            && let Some(destination_accounts) = destinations.get(&amount)
            && let [destination] = destination_accounts.as_slice()
        {
            mapping.insert(*source, *destination);
        }
    }
    Ok(mapping)
}

fn net_contributions(
    data: &JewelData,
    offering_id: i64,
    journal: &BTreeMap<i64, i64>,
) -> Result<BTreeMap<i64, Vec<PreparedContribution>>, JewelImportError> {
    let fund_mapping = contribution_fund_mapping(data, offering_id, journal)?;
    let mut contributions = BTreeMap::new();
    let mut offsets = BTreeMap::new();
    for envelope in data.envelopes.get(&offering_id).into_iter().flatten() {
        let can_offset = cents(envelope.cash_total)? >= 0 && cents(envelope.check_total)? >= 0;
        let mut entries = Vec::new();
        for contribution in data
            .contributions
            .get(&envelope.envelope_id)
            .into_iter()
            .flatten()
        {
            let amount = cents(contribution.amount)?;
            let account_id = fund_mapping
                .get(&contribution.account_id)
                .copied()
                .unwrap_or(contribution.account_id);
            if can_offset && amount != 0 {
                let (positive, negative) = offsets
                    .entry((envelope.name_id, account_id))
                    .or_insert((0_i64, 0_i64));
                let total = if amount > 0 { positive } else { negative };
                *total = total.checked_add(amount.abs()).ok_or_else(|| {
                    JewelImportError::InvalidData("Contribution total is too large".into())
                })?;
            }
            entries.push(PreparedContribution {
                account_id,
                amount,
                source_amount: amount,
            });
        }
        contributions.insert(envelope.envelope_id, entries);
    }

    // Cancel equal amounts on both sides within this offering, donor, and fund.
    // Negative cash/check envelopes remain separate bank withdrawals.
    for (positive, negative) in offsets.values_mut() {
        let amount = (*positive).min(*negative);
        *positive = amount;
        *negative = amount;
    }
    for envelope in data.envelopes.get(&offering_id).into_iter().flatten() {
        if cents(envelope.cash_total)? < 0 || cents(envelope.check_total)? < 0 {
            continue;
        }
        for contribution in contributions
            .get_mut(&envelope.envelope_id)
            .into_iter()
            .flatten()
        {
            if let Some((positive, negative)) =
                offsets.get_mut(&(envelope.name_id, contribution.account_id))
            {
                let remaining = if contribution.amount > 0 {
                    positive
                } else {
                    negative
                };
                let amount = (*remaining).min(contribution.amount.abs());
                if contribution.amount > 0 {
                    contribution.amount -= amount;
                } else {
                    contribution.amount += amount;
                }
                *remaining -= amount;
            }
        }
    }
    Ok(contributions)
}

fn extract_fund_reclassifications(
    data: &JewelData,
    offering_id: i64,
    contributions: &mut BTreeMap<i64, Vec<PreparedContribution>>,
) -> Result<Vec<FundReclassification>, JewelImportError> {
    let mut reclassifications = Vec::new();
    for envelope in data.envelopes.get(&offering_id).into_iter().flatten() {
        let cash = cents(envelope.cash_total)?;
        let check = cents(envelope.check_total)?;
        if cash < 0
            || check < 0
            || envelope
                .check_num
                .as_deref()
                .is_some_and(|number| number.trim().to_lowercase().ends_with(" reversal"))
        {
            continue;
        }
        let Some(entries) = contributions.get_mut(&envelope.envelope_id) else {
            continue;
        };
        let envelope_total = cash
            .checked_add(check)
            .ok_or_else(|| JewelImportError::InvalidData("Envelope total is too large".into()))?;
        let mut source_total = 0_i64;
        let mut fund_totals = BTreeMap::new();
        for entry in entries.iter() {
            source_total = source_total
                .checked_add(entry.source_amount)
                .ok_or_else(|| {
                    JewelImportError::InvalidData("Contribution total is too large".into())
                })?;
            add_balance(&mut fund_totals, entry.account_id, entry.amount)?;
        }
        if source_total != envelope_total {
            continue;
        }

        let mut sources = BTreeMap::<i64, Vec<i64>>::new();
        let mut destinations = BTreeMap::<i64, Vec<i64>>::new();
        for (account_id, amount) in fund_totals {
            if data.bank_accounts.contains(&account_id) {
                continue;
            }
            if amount < 0 {
                let amount = amount.checked_neg().ok_or_else(|| {
                    JewelImportError::InvalidData("Fund correction amount is too large".into())
                })?;
                sources.entry(amount).or_default().push(account_id);
            } else if amount > 0 {
                destinations.entry(amount).or_default().push(account_id);
            }
        }
        let mut funds = BTreeMap::new();
        for (amount, source_accounts) in sources {
            if let [source] = source_accounts.as_slice()
                && let Some(destination_accounts) = destinations.get(&amount)
                && let [destination] = destination_accounts.as_slice()
            {
                add_balance(&mut funds, *source, -amount)?;
                add_balance(&mut funds, *destination, amount)?;
                // Keep the correction's fund movements out of new donor giving.
                // Its equal positive and negative amounts have no bank effect.
                for entry in entries.iter_mut() {
                    if entry.account_id == *source || entry.account_id == *destination {
                        entry.amount = 0;
                    }
                }
            }
        }
        if !funds.is_empty() {
            reclassifications.push(FundReclassification {
                name_id: envelope.name_id,
                funds,
            });
        }
    }
    Ok(reclassifications)
}

impl OfferingConversion {
    pub fn balances_before_reversals(&self) -> Result<BTreeMap<i64, i64>, JewelImportError> {
        let mut balances = self.funds.clone();
        add_balance(&mut balances, self.bank_account_id, self.positive_total)?;
        for correction in &self.fund_reclassifications {
            for (&account_id, &amount) in &correction.funds {
                add_balance(&mut balances, account_id, amount)?;
            }
        }
        Ok(balances)
    }

    pub fn balances(&self) -> Result<BTreeMap<i64, i64>, JewelImportError> {
        if self.use_journal {
            return Ok(self.journal.clone());
        }
        let mut balances = self.balances_before_reversals()?;
        for reversal in &self.reversals {
            add_balance(&mut balances, self.bank_account_id, -reversal.total)?;
            for (&account_id, &amount) in &reversal.funds {
                add_balance(&mut balances, account_id, -amount)?;
            }
        }
        Ok(balances)
    }

    pub fn journal_difference(
        &self,
        imported: &BTreeMap<i64, i64>,
    ) -> Result<BTreeMap<i64, i64>, JewelImportError> {
        let mut difference = BTreeMap::new();
        for account_id in imported
            .keys()
            .chain(self.journal.keys())
            .copied()
            .collect::<BTreeSet<_>>()
        {
            let amount = self
                .journal
                .get(&account_id)
                .copied()
                .unwrap_or_default()
                .checked_sub(imported.get(&account_id).copied().unwrap_or_default())
                .ok_or_else(|| {
                    JewelImportError::InvalidData("Journal difference is too large".into())
                })?;
            difference.insert(account_id, amount);
        }
        Ok(difference)
    }
}

impl PreparedImport {
    pub fn new(data: JewelData) -> JournalResult<Self> {
        // Validate source roles before interpreting any signed journal or offering amounts.
        for (&account_id, account) in &data.accounts {
            if !account.active {
                continue;
            }
            let kind = AccountKind::try_from(account.account_type)?;
            if (kind == AccountKind::Bank) != data.bank_accounts.contains(&account_id) {
                return Err(JewelImportError::InvalidData(format!(
                    "Jewel account {account_id} has inconsistent AccountType and Accounts_Bank information"
                )).into());
            }
        }
        for (&journal_id, entries) in &data.journal_entries {
            for entry in entries {
                if cents(entry.amount)? != 0 {
                    let account = data
                        .accounts
                        .get(&entry.account_id)
                        .ok_or(JewelImportError::MissingAccount(entry.account_id))?;
                    if AccountKind::try_from(account.account_type)? == AccountKind::BeginningBalance
                    {
                        return Err(JewelImportError::InvalidData(format!(
                            "Beginning Balance account {} has a nonzero posting in Jewel journal {journal_id}", entry.account_id
                        )).into());
                    }
                }
            }
        }
        let opening_batches = opening_batches(&data)?;
        let mut deposit_journals = BTreeSet::new();
        let mut ignored_offerings = BTreeSet::new();
        for (&offering_id, offering) in &data.offerings {
            let journal = data.journals.get(&offering.deposit_journal_id).ok_or(
                JewelImportError::MissingJournalEntries(offering.deposit_journal_id),
            )?;
            if JournalKind::try_from(journal.journal_type_id)? != JournalKind::Deposit {
                return Err(JewelImportError::InvalidData(format!(
                    "Offering {offering_id} is linked to Jewel journal {}, which is not a deposit",
                    offering.deposit_journal_id
                ))
                .into());
            }
            if !deposit_journals.insert(offering.deposit_journal_id) {
                return Err(JewelImportError::InvalidData(format!(
                    "Several offerings share deposit journal {}",
                    offering.deposit_journal_id
                ))
                .into());
            }
            if cents(offering.offering_total)? == 0 {
                let mut contribution_total = 0_i64;
                for envelope in data.envelopes.get(&offering_id).into_iter().flatten() {
                    for contribution in data
                        .contributions
                        .get(&envelope.envelope_id)
                        .into_iter()
                        .flatten()
                    {
                        contribution_total = contribution_total
                            .checked_add(cents(contribution.amount)?)
                            .ok_or_else(|| {
                                JewelImportError::InvalidData(
                                    "Contribution total is too large".into(),
                                )
                            })?;
                    }
                }
                if contribution_total == 0 {
                    ignored_offerings.insert(offering_id);
                    continue;
                }
            }

            let mut envelope_total = 0_i64;
            for envelope in data.envelopes.get(&offering_id).into_iter().flatten() {
                for amount in [envelope.cash_total, envelope.check_total] {
                    envelope_total =
                        envelope_total.checked_add(cents(amount)?).ok_or_else(|| {
                            JewelImportError::InvalidData("Envelope total is too large".into())
                        })?;
                }
            }
            if envelope_total == 0 {
                let journal = data.journals.get(&offering.deposit_journal_id).ok_or(
                    JewelImportError::MissingJournalEntries(offering.deposit_journal_id),
                )?;
                let mut bank_total = 0_i64;
                for entry in data
                    .journal_entries
                    .get(&journal.journal_id)
                    .ok_or(JewelImportError::MissingJournalEntries(journal.journal_id))?
                {
                    if entry.account_id == journal.z_single_account_id {
                        bank_total =
                            bank_total
                                .checked_add(cents(entry.amount)?)
                                .ok_or_else(|| {
                                    JewelImportError::InvalidData("Bank total is too large".into())
                                })?;
                    }
                }
                // An apparently removed offering may retain a stale OfferingTotal.
                // Empty envelope and bank totals take precedence over that field.
                if bank_total == 0 {
                    ignored_offerings.insert(offering_id);
                }
            }
        }

        let mut conversions = BTreeMap::new();
        let mut issues = Vec::new();
        let mut reversal_count = 0;
        let mut fund_reclassification_count = 0;

        for (&offering_id, offering) in &data.offerings {
            if ignored_offerings.contains(&offering_id) {
                continue;
            }
            let journal = data.journals.get(&offering.deposit_journal_id).ok_or(
                JewelImportError::MissingJournalEntries(offering.deposit_journal_id),
            )?;
            let bank_account_id = journal.z_single_account_id;
            if !data.bank_accounts.contains(&bank_account_id) {
                return Err(JewelImportError::MissingAccount(bank_account_id).into());
            }
            let mut journal_balances = BTreeMap::new();
            for entry in data
                .journal_entries
                .get(&journal.journal_id)
                .ok_or(JewelImportError::MissingJournalEntries(journal.journal_id))?
            {
                add_balance(
                    &mut journal_balances,
                    entry.account_id,
                    cents(entry.amount)?,
                )?;
            }

            let mut positive_total = 0_i64;
            let mut funds = BTreeMap::new();
            let mut envelope_total = 0_i64;
            let mut contribution_total = 0_i64;
            let mut reversals = Vec::new();
            let mut problems = Vec::new();
            let mut has_contributions = false;
            let mut use_journal = offering.offering_source == 2;
            let mut contributions = net_contributions(&data, offering_id, &journal_balances)?;
            let fund_reclassifications = if offering.offering_source == 2 {
                Vec::new()
            } else {
                extract_fund_reclassifications(&data, offering_id, &mut contributions)?
            };

            for envelope in data.envelopes.get(&offering_id).into_iter().flatten() {
                let cash = cents(envelope.cash_total)?;
                let check = cents(envelope.check_total)?;
                use_journal |= cash < 0
                    || check < 0
                    || envelope
                        .check_num
                        .as_deref()
                        .is_some_and(|number| number.trim().to_lowercase().ends_with(" reversal"));
                let envelope_amount = cash.checked_add(check).ok_or_else(|| {
                    JewelImportError::InvalidData("Envelope amount is too large".into())
                })?;
                envelope_total = envelope_total.checked_add(envelope_amount).ok_or_else(|| {
                    JewelImportError::InvalidData("Envelope total is too large".into())
                })?;
                let mut envelope_contributions = 0_i64;
                let mut negative_funds = BTreeMap::new();
                let mut negative_total = 0_i64;
                let mut has_positive = false;
                for contribution in contributions
                    .get(&envelope.envelope_id)
                    .into_iter()
                    .flatten()
                {
                    has_contributions = true;
                    let amount = contribution.amount;
                    envelope_contributions = envelope_contributions
                        .checked_add(contribution.source_amount)
                        .ok_or_else(|| {
                            JewelImportError::InvalidData("Contribution total is too large".into())
                        })?;
                    if amount > 0 {
                        has_positive = true;
                        positive_total = positive_total.checked_add(amount).ok_or_else(|| {
                            JewelImportError::InvalidData("Donation total is too large".into())
                        })?;
                        add_balance(&mut funds, contribution.account_id, amount)?;
                    } else if amount < 0 {
                        add_balance(&mut negative_funds, contribution.account_id, -amount)?;
                        negative_total = negative_total.checked_sub(amount).ok_or_else(|| {
                            JewelImportError::InvalidData("Reversal total is too large".into())
                        })?;
                    }
                }
                contribution_total = contribution_total
                    .checked_add(envelope_contributions)
                    .ok_or_else(|| {
                        JewelImportError::InvalidData("Contribution total is too large".into())
                    })?;
                if envelope_amount != envelope_contributions {
                    problems.push(format!(
                        "Envelope {} does not match its contribution lines",
                        envelope.envelope_id
                    ));
                }
                if negative_total == 0 {
                    continue;
                }
                if has_positive {
                    problems.push(format!(
                        "Envelope {} has negative contribution lines that cannot be fully offset by positive contributions for the same donor and fund in this offering. Importing donor details keeps the remaining positive amounts in giving history.",
                        envelope.envelope_id
                    ));
                }

                reversals.push(Reversal {
                    envelope_id: envelope.envelope_id,
                    funds: negative_funds,
                    total: negative_total,
                });
            }

            let conversion = OfferingConversion {
                use_journal,
                bank_account_id,
                positive_total,
                funds,
                contributions,
                reversals,
                fund_reclassifications,
                journal: journal_balances,
                has_contributions,
            };
            let balances = conversion.balances()?;
            let differences: Vec<_> = balances
                .keys()
                .chain(conversion.journal.keys())
                .copied()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter_map(|account_id| {
                    let contributions = balances.get(&account_id).copied().unwrap_or_default();
                    let journal = conversion
                        .journal
                        .get(&account_id)
                        .copied()
                        .unwrap_or_default();
                    (contributions != journal).then(|| AccountDifference {
                        account_id,
                        name: data
                            .accounts
                            .get(&account_id)
                            .map(|account| account.name.clone())
                            .unwrap_or_else(|| format!("Account {account_id}")),
                        contributions,
                        journal,
                    })
                })
                .collect();
            let offering_total = cents(offering.offering_total)?;
            let bank_total = conversion
                .journal
                .get(&bank_account_id)
                .copied()
                .unwrap_or_default();
            if offering_total != bank_total {
                problems.push("OfferingTotal does not match the bank journal amount".into());
            }
            if offering_total != envelope_total {
                problems.push("OfferingTotal does not match the envelope total".into());
            }
            if !conversion.use_journal && (!differences.is_empty() || !problems.is_empty()) {
                issues.push(ReconciliationIssue {
                    offering_id,
                    date: offering.date.format("%Y-%m-%d").to_string(),
                    deposit_date: journal.date.format("%Y-%m-%d").to_string(),
                    offering_total,
                    envelope_total,
                    positive_contributions: positive_total,
                    contribution_total,
                    bank_total,
                    has_contributions,
                    differences,
                    problems,
                });
            }
            reversal_count += conversion.reversals.len();
            fund_reclassification_count += conversion.fund_reclassifications.len();
            conversions.insert(offering_id, conversion);
        }

        let report = ImportReport {
            accounts: data
                .accounts
                .values()
                .filter(|account| account.active && account.account_type != 0)
                .count(),
            donors: data.names.values().filter(|name| name.donor).count(),
            journals: data
                .journals
                .keys()
                .filter(|journal_id| {
                    !data
                        .offering_journals
                        .get(journal_id)
                        .is_some_and(|offering_id| ignored_offerings.contains(offering_id))
                })
                .count()
                - opening_batches
                    .values()
                    .map(|batch| batch.journal_count)
                    .sum::<usize>()
                + opening_batches.len(),
            offerings: data.offerings.len() - ignored_offerings.len(),
            reversals: reversal_count,
            fund_reclassifications: fund_reclassification_count,
            issues,
        };
        Ok(Self {
            data,
            opening_batches,
            report,
            conversions,
            ignored_offerings,
        })
    }

    pub fn validate_resolutions(&self, resolutions: &ImportResolutions) -> JournalResult<()> {
        for issue in &self.report.issues {
            let choice = resolutions
                .offerings
                .get(&issue.offering_id)
                .ok_or_else(|| {
                    JewelImportError::InvalidData(format!(
                        "Choose how to reconcile offering {}",
                        issue.offering_id
                    ))
                })?;
            if !issue.has_contributions && *choice != ReconciliationChoice::JournalOnly {
                return Err(JewelImportError::InvalidData(format!(
                    "Offering {} has no contribution details; preserve its journal transaction",
                    issue.offering_id
                ))
                .into());
            }
        }
        Ok(())
    }
}
