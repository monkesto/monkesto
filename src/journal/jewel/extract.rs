// allow regular sqlx functions as the macros are more trouble than they're worth here
#![allow(clippy::disallowed_methods)]

use sqlx::error::BoxDynError;
use sqlx::types::time::OffsetDateTime;
use sqlx::{Database, Decode, FromRow, SqliteConnection, Type};
use std::collections::BTreeMap;

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

pub struct JewelData {
    pub currency: JewelCurrency,
    pub accounts: BTreeMap<i64, JewelAccount>,
    pub names: BTreeMap<i64, JewelName>,
    pub offerings: Vec<JewelOffering>,
    pub contributions: BTreeMap<i64, JewelContribution>,
    pub envelopes: BTreeMap<i64, JewelEnvelope>,
    pub journals: BTreeMap<i64, JewelJournal>,
    pub journal_items: Vec<JewelJournalItem>,
}
pub async fn jewel_extract(conn: &mut SqliteConnection) -> Result<JewelData, sqlx::Error> {
    let currency: JewelCurrency = sqlx::query_scalar(
        r#"SELECT OptValue FROM Options WHERE OptName == "General.CurrencyName""#,
    )
    .fetch_one(&mut *conn)
    .await?;

    let accounts = sqlx::query_as(
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

    Ok(JewelData {
        currency,
        accounts,
        names,
        offerings,
        contributions,
        envelopes,
        journals,
        journal_items,
    })
}
