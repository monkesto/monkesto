// allow regular sqlx functions as the macros are more trouble than they're worth here
#![allow(clippy::disallowed_methods)]

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use sqlx::error::BoxDynError;
use sqlx::types::time::OffsetDateTime;
use sqlx::{Database, Decode, FromRow, SqliteConnection, Type};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap};
use std::str::FromStr;
use thiserror::Error;

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug)]
pub enum JewelCurrency {
    USD,
    #[expect(unused)]
    Unknown(String),
}

impl<DB: Database> Type<DB> for JewelCurrency
where
    String: sqlx::Type<DB>,
{
    fn type_info() -> <DB>::TypeInfo {
        <String as Type<DB>>::type_info()
    }
}

impl<'r, DB: Database> Decode<'r, DB> for JewelCurrency
where
    String: Decode<'r, DB>,
{
    fn decode(value: <DB>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let str = String::decode(value)?;
        Ok(match str.as_str() {
            "Dollars" => Self::USD,
            _ => JewelCurrency::Unknown(str),
        })
    }
}

#[derive(Debug, FromRow)]
#[expect(unused)]
#[sqlx(rename_all = "PascalCase")]
pub struct JewelAccount {
    #[sqlx(rename = "AccountID")]
    account_id: i64,
    /// mystery int
    account_type: i64,
    pub name: String,
    #[sqlx(rename = "ParentAccountID")]
    parent_id: Option<i64>,
    tax_deductible: bool,
    // allow_posting?
    local_income: bool,
    local_expense: bool,
    permanent: bool,
    active: bool,
}

#[expect(unused)]
#[derive(Debug, FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct JewelName {
    #[sqlx(rename = "NameID")]
    name_id: i64,
    pub name: String,
    last_name: String,
    first_name: Option<String>,
    address: Option<String>,
    cell_phone: Option<String>,
    home_phone: Option<String>,
    work_phone: Option<String>,
    email_address: Option<String>,
    get_receipt: bool,
    donor: bool,
    active: bool,
}

#[expect(unused)]
#[derive(Debug, FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct JewelOffering {
    #[sqlx(rename = "OfferingID")]
    offering_id: i64,
    date: OffsetDateTime,
    /// yes, jewel stores money as a floating point number
    offering_total: f64,
    #[sqlx(rename = "DepositJournalID")]
    deposit_journal_id: i64,

    // no idea what these do
    #[sqlx(rename = "ArchiveID")]
    archive_id: i64,
    offering_source: i64,
}

#[expect(unused)]
#[derive(Debug, FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct JewelContribution {
    #[sqlx(rename = "ContribID")]
    contribution_id: i64,

    #[sqlx(rename = "EnvID")]
    pub envelope_id: i64,

    #[sqlx(rename = "AccountID")]
    pub account_id: i64,

    /// again, jewel stores money with *floats*
    amount: f64,
}

#[expect(unused)]
#[derive(Debug, FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct JewelEnvelope {
    #[sqlx(rename = "EnvID")]
    envelope_id: i64,

    #[sqlx(rename = "OfferingID")]
    offering_id: i64,

    #[sqlx(rename = "NameID")]
    pub name_id: i64,

    /// yes, floating point money
    cash_total: f64,

    /// yes, floating point money
    check_total: f64,

    /// Envelopes are also created for check reversals
    ///
    /// A reversal will be in the form `{original_num} reversal` and have a negative `CheckTotal`
    ///
    /// Check numbers are alphanumeric even if there isn't a reversal
    check_num: Option<String>,

    check_reversed: bool,
}

#[expect(unused)]
#[derive(Debug, FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct JewelJournal {
    #[sqlx(rename = "JournalID")]
    journal_id: i64,
    accounting_date: OffsetDateTime,

    /// mystery int
    #[sqlx(rename = "JournalTypeID")]
    journal_type_id: i64,

    #[sqlx(rename = "SeqNum")]
    sequence_number: i64,
    date: OffsetDateTime,

    #[sqlx(rename = "VendorID")]
    vendor_id: Option<i64>,
    pub memo: String,

    #[sqlx(rename = "zSingleAccountID")]
    z_single_account_id: i64,
}

#[expect(unused)]
#[derive(Debug, FromRow)]
#[sqlx(rename_all = "PascalCase")]
pub struct JewelJournalItem {
    #[sqlx(rename = "JournalItemID")]
    journal_item_id: i64,

    #[sqlx(rename = "JournalID")]
    pub journal_id: i64,

    #[sqlx(rename = "AccountID")]
    pub account_id: i64,

    /// yes, floating point money
    amount: f64,
}

pub struct StandardJewelStats {
    /// monthly giving to the church budget fund in the last x months (inclusive), ordered most to least recent
    pub monthly_budget_giving: Vec<i64>,

    /// monthly tithe received in the last x months (inclusive), ordered most to least recent
    pub monthly_tithe: Vec<i64>,

    /// amount and name of the top x expenses in the last x months, ordered most to least recent
    pub monthly_top_expenses: Vec<Vec<(i64, String)>>,

    /// (online, in_house, unknown) giving combined tithe and budget for the last x months, ordered most to least recent
    pub monthly_online_vs_offline_giving: Vec<(i64, i64, i64)>,
}

pub enum OfferingSource {
    InPerson,
    Online,
    Unknown,
    ReturnedCheck,
}

#[derive(Debug, Error)]
#[error("invalid jewel offering source: {0}")]
pub struct OfferingSourceDecodeError(i64);

impl sqlx::Type<sqlx::Sqlite> for OfferingSource {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <i64 as sqlx::Type<sqlx::Sqlite>>::type_info()
    }
}
impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for OfferingSource {
    fn decode(value: <sqlx::Sqlite as Database>::ValueRef<'r>) -> Result<Self, BoxDynError> {
        let source: i64 = Decode::<sqlx::Sqlite>::decode(value)?;

        match source {
            0 => Ok(OfferingSource::Unknown),
            1 => Ok(OfferingSource::InPerson),
            2 => Ok(OfferingSource::ReturnedCheck),
            3 | 4 => Ok(OfferingSource::Unknown),
            5 => Ok(OfferingSource::Online),
            _ => Err(OfferingSourceDecodeError(source).into()),
        }
    }
}

#[expect(unused)]
pub struct JewelData {
    pub currency: JewelCurrency,
    pub accounts: BTreeMap<i64, JewelAccount>,
    pub names: BTreeMap<i64, JewelName>,
    pub offerings: Vec<JewelOffering>,
    pub contributions: BTreeMap<i64, JewelContribution>,
    pub envelopes: BTreeMap<i64, JewelEnvelope>,
    pub journals: BTreeMap<i64, JewelJournal>,
    pub journal_items: Vec<JewelJournalItem>,
    pub standard_stats: StandardJewelStats,
}
pub async fn jewel_extract(
    conn: &mut SqliteConnection,
    num_months: u32,
    num_top_expenses: usize,
) -> Result<JewelData, sqlx::Error> {
    let currency: JewelCurrency = sqlx::query_scalar(
        r#"SELECT OptValue FROM Options WHERE OptName == "General.CurrencyName""#,
    )
    .fetch_one(&mut *conn)
    .await?;

    let accounts: BTreeMap<i64, JewelAccount> = sqlx::query_as(
        r#"
        SELECT AccountID,
               AccountType,
               Name,
               ParentAccountID,
               TaxDeductible,
               LocalIncome,
               LocalExpense,
               Permanent,
               Active
        FROM Accounts
    "#,
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|acc: JewelAccount| (acc.account_id, acc))
    .collect();

    let names = sqlx::query_as(
        r#"
        SELECT NameID,
               Name,
               LastName,
               FirstName,
               Address,
               CellPhone,
               HomePhone,
               WorkPhone,
               EmailAddress,
               GetReceipt,
               Donor,
               Active
       FROM Names
    "#,
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|name: JewelName| (name.name_id, name))
    .collect();

    let offerings = sqlx::query_as(
        r#"
        SELECT OfferingID,
               Date,
               OfferingTotal,
               DepositJournalID,
               ArchiveId,
               OfferingSource
        FROM Offerings
        "#,
    )
    .fetch_all(&mut *conn)
    .await?;

    let contributions = sqlx::query_as(
        r#"
        SELECT ContribID,
               EnvID,
               AccountID,
               Amount
        FROM Contributions
            "#,
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|contribution: JewelContribution| (contribution.contribution_id, contribution))
    .collect();

    let envelopes = sqlx::query_as(
        r#"
        SELECT EnvID,
               OfferingID,
               NameID,
               CashTotal,
               CheckTotal,
               CheckNum,
               CheckReversed
        FROM Envelopes
            "#,
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|envelope: JewelEnvelope| (envelope.envelope_id, envelope))
    .collect();

    let journals = sqlx::query_as(
        r#"
        SELECT JournalID,
               AccountingDate,
               JournalTypeID,
               SeqNum,
               Date,
               VendorID,
               Memo,
               ZSingleAccountID
       FROM Journal
            "#,
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|journal: JewelJournal| (journal.journal_id, journal))
    .collect();

    let journal_items = sqlx::query_as(
        r#"
            SELECT JournalItemID,
                   JournalID,
                   AccountID,
                   Amount
            FROM JournalItems
            "#,
    )
    .fetch_all(&mut *conn)
    .await?;

    // statistics

    let church_budget_account_id_str: String = sqlx::query_scalar(
        r#"
            SELECT OptValue FROM Options WHERE OptName == "AutoAccounts.Account2"
            "#,
    )
    .fetch_one(&mut *conn)
    .await?;

    let tithe_account_id_str: String = sqlx::query_scalar(
        r#"
            SELECT OptValue FROM Options WHERE OptName == "AutoAccounts.Account1"
            "#,
    )
    .fetch_one(&mut *conn)
    .await?;

    let church_budget_account_id = i64::from_str(church_budget_account_id_str.trim())
        .expect("failed to parse church budget ID");
    let tithe_account_id =
        i64::from_str(tithe_account_id_str.trim()).expect("failed to parse tithe account ID");

    let now = Utc::now().date_naive();

    let mut start_year = now.year() - (num_months as i32 / 12);

    let month_offset = num_months % 12;

    let start_month = if now.month() < month_offset {
        start_year -= 1;
        now.month() + (month_offset - now.month())
    } else {
        now.month() - month_offset
    };

    let start_cutoff = NaiveDate::from_ymd_opt(start_year, start_month, 1)
        .expect("valid date")
        .format("%Y-%m-%d")
        .to_string();

    // (id, date, total, source)
    let offerings_last_x_months: Vec<(i64, DateTime<Utc>, f64, OfferingSource)> = sqlx::query_as(
        r#"
            SELECT OfferingID, Date, OfferingTotal, OfferingSource from Offerings
                WHERE Date >= ?
                ORDER BY Date DESC
            "#,
    )
    .bind(start_cutoff.as_str())
    .fetch_all(&mut *conn)
    .await?;

    let last_x_month_offerings = {
        let current_date = Utc::now();

        let mut month_idx = current_date.month();
        let mut year_idx = current_date.year();

        let mut all_months = Vec::new();

        let mut current_month_offerings = Vec::new();

        for (id, offering_date, total, source) in offerings_last_x_months {
            // round to the nearest cent, if necessary
            let total_cents = (total * 100.0).round() as i64;

            if offering_date.year() == year_idx && offering_date.month() == month_idx {
                current_month_offerings.push((id, offering_date, total_cents, source));
            } else {
                all_months.push(current_month_offerings);
                current_month_offerings = vec![(id, offering_date, total_cents, source)];

                if month_idx > 1 {
                    month_idx -= 1;
                } else {
                    month_idx = 12;
                    year_idx -= 1;
                }
            }
        }

        all_months
    };

    let mut monthly_budget_giving: Vec<i64> = Vec::new();
    let mut monthly_tithe: Vec<i64> = Vec::new();

    // (in_person, online, unknown)
    let mut monthly_online_vs_offline_giving: Vec<(i64, i64, i64)> = Vec::new();

    for month in last_x_month_offerings {
        let mut month_budget_giving = 0;
        let mut month_tithe_giving = 0;

        let mut month_in_person_giving = 0;
        let mut month_online_giving = 0;
        let mut month_unknown_giving = 0;

        for (offering_id, _, total, source) in month {
            let budget_giving_envelopes: Vec<f64> = sqlx::query_scalar(
                r#"
                    SELECT c.Amount
                    FROM Contributions c
                    JOIN Envelopes e ON c.EnvID = e.EnvID
                    WHERE c.AccountID = ?
                      AND e.OfferingID = ?
                    "#,
            )
            .bind(church_budget_account_id)
            .bind(offering_id)
            .fetch_all(&mut *conn)
            .await?;

            let tithe_giving_envelopes: Vec<f64> = sqlx::query_scalar(
                r#"
                    SELECT c.Amount
                    FROM Contributions c
                    JOIN Envelopes e ON c.EnvID = e.EnvID
                    WHERE c.AccountID = ?
                      AND e.OfferingID = ?
                    "#,
            )
            .bind(tithe_account_id)
            .bind(offering_id)
            .fetch_all(&mut *conn)
            .await?;

            for amount in budget_giving_envelopes {
                month_budget_giving += (amount * 100.0).round() as i64;
            }

            for amount in tithe_giving_envelopes {
                month_tithe_giving += (amount * 100.0).round() as i64;
            }

            match source {
                OfferingSource::InPerson => month_in_person_giving += total,
                OfferingSource::Online => month_online_giving += total,
                OfferingSource::Unknown => month_unknown_giving += total,
                OfferingSource::ReturnedCheck => {}
            }
        }

        monthly_budget_giving.push(month_budget_giving);
        monthly_tithe.push(month_tithe_giving);
        monthly_online_vs_offline_giving.push((
            month_in_person_giving,
            month_online_giving,
            month_unknown_giving,
        ));
    }

    // (journal_id, zsingle_account_id (bank account id?), date)
    let expense_journals_last_x_months: Vec<(i64, i64, DateTime<Utc>)> = sqlx::query_as(
        r#"
            SELECT JournalID, zSingleAccountId, AccountingDate FROM Journal
                WHERE AccountingDate >= ? AND JournalTypeID = 4
                ORDER BY Date DESC
            "#,
    )
    .bind(start_cutoff.as_str())
    .fetch_all(&mut *conn)
    .await?;

    let last_x_month_expense_journals = {
        let current_date = Utc::now();

        let mut month_idx = current_date.month();
        let mut year_idx = current_date.year();

        let mut all_months = Vec::new();

        let mut current_month_journals = Vec::new();

        for (id, zsingle_account_id, accounting_date) in expense_journals_last_x_months {
            if accounting_date.year() == year_idx && accounting_date.month() == month_idx {
                current_month_journals.push((id, zsingle_account_id));
            } else {
                all_months.push(current_month_journals);
                current_month_journals = vec![(id, zsingle_account_id)];

                if month_idx > 1 {
                    month_idx -= 1;
                } else {
                    month_idx = 12;
                    year_idx -= 1;
                }
            }
        }

        all_months
    };

    let mut monthly_top_expenses = Vec::new();

    for month in last_x_month_expense_journals {
        let mut month_all_expenses = HashMap::new();

        for (journal_id, zsingle_account_id) in month {
            let journal_items: Vec<(i64, f64)> = sqlx::query_as(
                r#"
                    SELECT AccountId, Amount FROM JournalItems
                        WHERE JournalID = ? AND AccountID != ? AND AccountID != ?
                    "#,
            )
            .bind(journal_id)
            .bind(zsingle_account_id)
            .bind(tithe_account_id)
            .fetch_all(&mut *conn)
            .await?;

            for (account_id, amount) in journal_items {
                // jewel stores all debit entries as negative floating point numbers
                let amount_cents = -(amount * 100.0).round() as i64;

                month_all_expenses
                    .entry(account_id)
                    .and_modify(|c| *c += amount_cents)
                    .or_insert(amount_cents);
            }
        }

        // sort for the top x entries
        let mut heap = BinaryHeap::with_capacity(num_top_expenses + 1);
        for (account_id, amount) in month_all_expenses {
            heap.push((Reverse(amount), account_id));
            if heap.len() > num_top_expenses {
                heap.pop();
            }
        }

        let mut top_expenses: Vec<(i64, String)> = Vec::with_capacity(num_top_expenses);

        for (Reverse(amount), account_id) in heap.into_iter() {
            top_expenses.push((
                amount,
                accounts
                    .get(&account_id)
                    .map(|account| account.name.clone())
                    .unwrap_or("unknown account".to_string()),
            ));
        }

        top_expenses.sort_unstable_by_key(|a| Reverse(a.0));
        monthly_top_expenses.push(top_expenses);
    }

    let standard_stats = StandardJewelStats {
        monthly_budget_giving,
        monthly_tithe,
        monthly_online_vs_offline_giving,
        monthly_top_expenses,
    };

    Ok(JewelData {
        currency,
        accounts,
        names,
        offerings,
        contributions,
        envelopes,
        journals,
        journal_items,
        standard_stats,
    })
}
