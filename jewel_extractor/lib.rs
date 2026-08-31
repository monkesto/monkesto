use sqlx::SqliteConnection;
use std::collections::BTreeMap;

#[derive(Debug)]
#[expect(unused)]
pub struct JewelAccount {
    account_id: i64,
    /// mystery int
    account_type: i64,
    name: String,
    parent_id: Option<i64>,
    tax_deductible: bool,
    // allow_posting?
    local_income: bool,
    local_expense: bool,
    permanent: bool,
    active: bool,
}

#[expect(unused)]
#[derive(Debug)]
pub struct JewelName {
    name_id: i64,
    name: String,
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

pub struct JewelData {
    pub accounts: BTreeMap<i64, JewelAccount>,
    pub names: BTreeMap<i64, JewelName>,
}
pub async fn jewel_extract(conn: &mut SqliteConnection) -> Result<JewelData, sqlx::Error> {
    let accounts = sqlx::query_as!(
        JewelAccount,
        r#"
        select AccountID as account_id,
               AccountType as account_type,
               Name as name,
               ParentAccountID as parent_id,
               TaxDeductible as "tax_deductible: bool",
               LocalIncome as "local_income: bool",
               LocalExpense as "local_expense: bool",
               Permanent as "permanent: bool",
               Active as "active: bool"
        FROM Accounts
    "#
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|acc| (acc.account_id, acc))
    .collect();

    let names = sqlx::query_as!(
        JewelName,
        r#"
        select NameID as name_id,
               Name as "name!",
               LastName as "last_name!",
               FirstName as first_name,
               Address as address,
               CellPhone as cell_phone,
               HomePhone as home_phone,
               WorkPhone as work_phone,
               EmailAddress as email_address,
               GetReceipt as "get_receipt: bool",
               Donor as "donor: bool",
               Active as "active: bool"
       FROM Names
    "#
    )
    .fetch_all(&mut *conn)
    .await?
    .into_iter()
    .map(|name| (name.name_id, name))
    .collect();

    Ok(JewelData { accounts, names })
}
