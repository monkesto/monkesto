use crate::account::AccountStore;
use crate::auth::user::Email;
use crate::auth::user::UserStore;
use crate::authority::UserId;
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::journal::JournalStore;
use crate::journal::Permissions;
use crate::known_errors::KnownErrors;
use crate::service::Service;
use crate::transaction::BalanceUpdate;
use crate::transaction::EntryType;
use crate::transaction::TransactionStore;
use std::str::FromStr;

pub(crate) async fn seed_dev_data<U, J, T, A>(
    service: &Service<U, J, T, A>,
) -> Result<(), KnownErrors>
where
    U: UserStore,
    J: JournalStore,
    T: TransactionStore,
    A: AccountStore,
{
    // TODO: Unify user seeding

    service
        .user_store()
        .seed_dev_users()
        .await
        .map_err(|e| KnownErrors::InternalError {
            context: e.to_string(),
        })?;

    let pacioli_id = UserId::from_str("zk8m3p5q7r2n4v6x")?;
    let wedgwood_id = UserId::from_str("yj7l2o4p6q8s0u1w")?;

    let wedgwood_email = Email::try_new(
        service
            .user_get_email(wedgwood_id)
            .await?
            .ok_or(KnownErrors::UserDoesntExist)?,
    )
    .map_err(|e| KnownErrors::InternalError {
        context: e.to_string(),
    })?;

    let maple_ridge_academy_id = JournalId::from_str("ab1cd2ef3g")?;
    let smith_and_sons_id = JournalId::from_str("hi4jk5lm6n")?;
    let green_valley_id = JournalId::from_str("op7qr8st9u")?;

    let assets_id = AccountId::from_str("ac1assets0")?;
    let revenue_id = AccountId::from_str("ac4revenue")?;
    let expenses_id = AccountId::from_str("ac5expense")?;

    if service
        .journal_get(maple_ridge_academy_id, pacioli_id)
        .await?
        .is_none()
    {
        service
            .journal_create(
                maple_ridge_academy_id,
                "Maple Ridge Academy".to_owned(),
                pacioli_id,
            )
            .await?;
    }

    if service
        .journal_get(smith_and_sons_id, pacioli_id)
        .await?
        .is_none()
    {
        service
            .journal_create(
                JournalId::from_str("hi4jk5lm6n")?,
                "Smith & Sons Bakery".to_owned(),
                pacioli_id,
            )
            .await?;
    }

    if service
        .journal_get(green_valley_id, pacioli_id)
        .await?
        .is_none()
    {
        service
            .journal_create(
                JournalId::from_str("op7qr8st9u")?,
                "Green Valley Farm Co.".to_owned(),
                pacioli_id,
            )
            .await?;
    }

    // journal_get returns none if the actor isn't a tenant
    if service
        .journal_get(maple_ridge_academy_id, wedgwood_id)
        .await?
        .is_none()
    {
        service
            .journal_invite_tenant(
                maple_ridge_academy_id,
                pacioli_id,
                wedgwood_email,
                Permissions::READ | Permissions::APPENDTRANSACTION,
            )
            .await?;
    }

    // working under the assumption that the presence of any accounts shows that they were already seeded
    // if this is proven to be false, iter().any() is available.
    if service
        .account_get_all_in_journal(maple_ridge_academy_id, pacioli_id)
        .await?
        .is_empty()
    {
        // Level 1: root accounts
        service
            .account_create(
                assets_id,
                maple_ridge_academy_id,
                pacioli_id,
                "Assets".to_owned(),
                None,
            )
            .await?;
        service
            .account_create(
                AccountId::from_str("ac2liabili")?,
                maple_ridge_academy_id,
                pacioli_id,
                "Liabilities".to_owned(),
                None,
            )
            .await?;
        service
            .account_create(
                AccountId::from_str("ac3equity0")?,
                maple_ridge_academy_id,
                pacioli_id,
                "Equity".to_owned(),
                None,
            )
            .await?;
        service
            .account_create(
                revenue_id,
                maple_ridge_academy_id,
                pacioli_id,
                "Revenue".to_owned(),
                None,
            )
            .await?;
        service
            .account_create(
                expenses_id,
                maple_ridge_academy_id,
                pacioli_id,
                "Expenses".to_owned(),
                None,
            )
            .await?;

        // Level 2: children of Assets
        let current_assets_id = AccountId::from_str("ac1current")?;
        service
            .account_create(
                current_assets_id,
                maple_ridge_academy_id,
                pacioli_id,
                "Current Assets".to_owned(),
                Some(assets_id),
            )
            .await?;
        service
            .account_create(
                AccountId::from_str("ac1fixedat")?,
                maple_ridge_academy_id,
                pacioli_id,
                "Fixed Assets".to_owned(),
                Some(assets_id),
            )
            .await?;

        // Level 2: children of Expenses
        let operating_exp_id = AccountId::from_str("ac5opexpen")?;
        service
            .account_create(
                operating_exp_id,
                maple_ridge_academy_id,
                pacioli_id,
                "Operating Expenses".to_owned(),
                Some(expenses_id),
            )
            .await?;
        service
            .account_create(
                AccountId::from_str("ac5capexp0")?,
                maple_ridge_academy_id,
                pacioli_id,
                "Capital Expenses".to_owned(),
                Some(expenses_id),
            )
            .await?;

        // Level 3: children of Current Assets
        let cash_id = AccountId::from_str("ac1cash000")?;
        service
            .account_create(
                cash_id,
                maple_ridge_academy_id,
                pacioli_id,
                "Cash".to_owned(),
                Some(current_assets_id),
            )
            .await?;
        service
            .account_create(
                AccountId::from_str("ac1recvabl")?,
                maple_ridge_academy_id,
                pacioli_id,
                "Accounts Receivable".to_owned(),
                Some(current_assets_id),
            )
            .await?;

        // Level 3: children of Operating Expenses
        let staffing_id = AccountId::from_str("ac5staffng")?;
        service
            .account_create(
                staffing_id,
                maple_ridge_academy_id,
                pacioli_id,
                "Staffing".to_owned(),
                Some(operating_exp_id),
            )
            .await?;
        service
            .account_create(
                AccountId::from_str("ac5suppls0")?,
                maple_ridge_academy_id,
                pacioli_id,
                "Supplies".to_owned(),
                Some(operating_exp_id),
            )
            .await?;

        // Level 4: children of Cash
        service
            .account_create(
                AccountId::from_str("ac1chkng00")?,
                maple_ridge_academy_id,
                pacioli_id,
                "Checking".to_owned(),
                Some(cash_id),
            )
            .await?;
        service
            .account_create(
                AccountId::from_str("ac1savngs0")?,
                maple_ridge_academy_id,
                pacioli_id,
                "Savings".to_owned(),
                Some(cash_id),
            )
            .await?;

        // Level 4: children of Staffing
        service
            .account_create(
                AccountId::from_str("ac5salrys0")?,
                maple_ridge_academy_id,
                pacioli_id,
                "Salaries".to_owned(),
                Some(staffing_id),
            )
            .await?;
        service
            .account_create(
                AccountId::from_str("ac5bnfts00")?,
                maple_ridge_academy_id,
                pacioli_id,
                "Benefits".to_owned(),
                Some(staffing_id),
            )
            .await?;
    }

    let checking_id = AccountId::from_str("ac1chkng00")?;
    let savings_id = AccountId::from_str("ac1savngs0")?;
    let salaries_id = AccountId::from_str("ac5salrys0")?;
    let supplies_id = AccountId::from_str("ac5suppls0")?;

    // again, the presence of any transactions should show that they were already seeded
    if service
        .transaction_get_all_in_journal(maple_ridge_academy_id, pacioli_id)
        .await?
        .is_empty()
    {
        // Transaction 1: Tuition deposited into checking - $5,000
        // Assets › Current Assets › Cash › Checking  /  Revenue
        service
            .transaction_create(
                TransactionId::from_str("t1tuition0000001")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: checking_id,
                        amount: 500000,
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: revenue_id,
                        amount: 500000,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;

        // Transaction 2: Teacher salaries paid from checking - $3,200
        // Expenses › Operating Expenses › Staffing › Salaries  /  Assets › Current Assets › Cash › Checking
        service
            .transaction_create(
                TransactionId::from_str("t2salary00000002")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: salaries_id,
                        amount: 320000,
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: checking_id,
                        amount: 320000,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;

        // Transaction 3: Classroom supplies paid from checking - $850
        // Expenses › Operating Expenses › Supplies  /  Assets › Current Assets › Cash › Checking
        service
            .transaction_create(
                TransactionId::from_str("t3textbooks00003")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: supplies_id,
                        amount: 85000,
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: checking_id,
                        amount: 85000,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;

        // Transaction 4: Tuition deposited into checking - $4,500
        // Assets › Current Assets › Cash › Checking  /  Revenue
        service
            .transaction_create(
                TransactionId::from_str("t4tuition0000004")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: checking_id,
                        amount: 450000,
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: revenue_id,
                        amount: 450000,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;

        // Transaction 5: Transfer from checking to savings - $2,000
        // Assets › Current Assets › Cash › Savings  /  Assets › Current Assets › Cash › Checking
        service
            .transaction_create(
                TransactionId::from_str("t5supplies000005")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: savings_id,
                        amount: 200000,
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: checking_id,
                        amount: 200000,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;

        // Transaction 6: Benefits paid from checking - $640
        // Expenses › Operating Expenses › Staffing › Benefits  /  Assets › Current Assets › Cash › Checking
        let benefits_id = AccountId::from_str("ac5bnfts00")?;
        service
            .transaction_create(
                TransactionId::from_str("t6chkdeposit0006")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: benefits_id,
                        amount: 64000,
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: checking_id,
                        amount: 64000,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;
    }

    Ok(())
}
