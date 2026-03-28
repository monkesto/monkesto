use crate::AppState;
use crate::auth::user::Email;
use crate::auth::user::UserStore;
use crate::auth::user::UserStoreError;
use crate::authority::UserId;
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::journal::Permissions;
use crate::monkesto_error::MonkestoResult;
use crate::name::Name;
use crate::transaction::BalanceUpdate;
use crate::transaction::EntryType;
use std::str::FromStr;

pub(crate) async fn seed_dev_data(service: &AppState) -> MonkestoResult<()> {
    // TODO: Unify user seeding

    service.user_service.store().seed_dev_users().await?;

    let pacioli_id = UserId::from_str("zk8m3p5q7r2n4v6x")?;
    let wedgwood_id = UserId::from_str("yj7l2o4p6q8s0u1w")?;

    let wedgwood_email = Email::try_new(
        service
            .user_service
            .user_get_email(wedgwood_id)
            .await?
            .ok_or(UserStoreError::UserNotFound)?,
    )?;

    let maple_ridge_academy_id = JournalId::from_str("ab1cd2ef3g")?;
    let smith_and_sons_id = JournalId::from_str("hi4jk5lm6n")?;
    let green_valley_id = JournalId::from_str("op7qr8st9u")?;

    let assets_id = AccountId::from_str("ac1assets0")?;
    let revenue_id = AccountId::from_str("ac4revenue")?;
    let expenses_id = AccountId::from_str("ac5expense")?;

    if service
        .journal_service
        .get_journal(maple_ridge_academy_id, pacioli_id)
        .await?
        .is_none()
    {
        service
            .journal_service
            .create_journal(
                maple_ridge_academy_id,
                Name::try_new("Maple Ridge Academy".to_string())
                    .expect("Failed to create a name from \"Maple Ridge Academy\""),
                pacioli_id,
            )
            .await?;
    }

    if service
        .journal_service
        .get_journal(smith_and_sons_id, pacioli_id)
        .await?
        .is_none()
    {
        service
            .journal_service
            .create_journal(
                JournalId::from_str("hi4jk5lm6n")?,
                Name::try_new("Smith & Sons Bakery".to_string())
                    .expect("Failed to create a name from \"Smith & Sons Bakery\""),
                pacioli_id,
            )
            .await?;
    }

    if service
        .journal_service
        .get_journal(green_valley_id, pacioli_id)
        .await?
        .is_none()
    {
        service
            .journal_service
            .create_journal(
                JournalId::from_str("op7qr8st9u")?,
                Name::try_new("Green Valley Farm Co.".to_string())
                    .expect("Failed to create a name from \"Green Valley Farm Co.\""),
                pacioli_id,
            )
            .await?;
    }

    // journal_get returns none if the actor isn't a tenant
    if service
        .journal_service
        .get_journal(maple_ridge_academy_id, wedgwood_id)
        .await?
        .is_none()
    {
        service
            .journal_service
            .journal_invite_member(
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
        .account_service
        .account_get_all_in_journal(maple_ridge_academy_id, pacioli_id)
        .await?
        .is_empty()
    {
        // Level 1: root accounts
        service
            .account_service
            .account_create(
                assets_id,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Assets".to_string())
                    .expect("Failed to create a name from \"Assets\""),
                None,
            )
            .await?;
        service
            .account_service
            .account_create(
                AccountId::from_str("ac2liabili")?,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Liabilities".to_string())
                    .expect("Failed to create a name from \"Liabilities\""),
                None,
            )
            .await?;
        service
            .account_service
            .account_create(
                AccountId::from_str("ac3equity0")?,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Equity".to_string())
                    .expect("Failed to create a name from \"Equity\""),
                None,
            )
            .await?;
        service
            .account_service
            .account_create(
                revenue_id,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Revenue".to_string())
                    .expect("Failed to create a name from \"Revenue\""),
                None,
            )
            .await?;
        service
            .account_service
            .account_create(
                expenses_id,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Expenses".to_string())
                    .expect("Failed to create a name from \"Expenses\""),
                None,
            )
            .await?;

        // Level 2: children of Assets
        let current_assets_id = AccountId::from_str("ac1current")?;
        service
            .account_service
            .account_create(
                current_assets_id,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Current Assets".to_string())
                    .expect("Failed to create a name from \"Current Assets\""),
                Some(assets_id),
            )
            .await?;
        service
            .account_service
            .account_create(
                AccountId::from_str("ac1fixedat")?,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Fixed Assets".to_string())
                    .expect("Failed to create a name from \"Fixed Assets\""),
                Some(assets_id),
            )
            .await?;

        // Level 2: children of Expenses
        let operating_exp_id = AccountId::from_str("ac5opexpen")?;
        service
            .account_service
            .account_create(
                operating_exp_id,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Operating Expenses".to_string())
                    .expect("Failed to create a name from \"Operating Expenses\""),
                Some(expenses_id),
            )
            .await?;
        service
            .account_service
            .account_create(
                AccountId::from_str("ac5capexp0")?,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Capital Expenses".to_string())
                    .expect("Failed to create a name from \"Capital Expenses\""),
                Some(expenses_id),
            )
            .await?;

        // Level 3: children of Current Assets
        let cash_id = AccountId::from_str("ac1cash000")?;
        service
            .account_service
            .account_create(
                cash_id,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Cash".to_string()).expect("Failed to create a name from \"Cash\""),
                Some(current_assets_id),
            )
            .await?;
        service
            .account_service
            .account_create(
                AccountId::from_str("ac1recvabl")?,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Accounts Receivable".to_string())
                    .expect("Failed to create a name from \"Accounts Receivable\""),
                Some(current_assets_id),
            )
            .await?;

        // Level 3: children of Operating Expenses
        let staffing_id = AccountId::from_str("ac5staffng")?;
        service
            .account_service
            .account_create(
                staffing_id,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Staffing".to_string())
                    .expect("Failed to create a name from \"Staffing\""),
                Some(operating_exp_id),
            )
            .await?;
        service
            .account_service
            .account_create(
                AccountId::from_str("ac5suppls0")?,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Supplies".to_string())
                    .expect("Failed to create a name from \"Supplies\""),
                Some(operating_exp_id),
            )
            .await?;

        // Level 4: children of Cash
        service
            .account_service
            .account_create(
                AccountId::from_str("ac1chkng00")?,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Checking".to_string())
                    .expect("Failed to create a name from \"Checking\""),
                Some(cash_id),
            )
            .await?;
        service
            .account_service
            .account_create(
                AccountId::from_str("ac1savngs0")?,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Savings".to_string())
                    .expect("Failed to create a name from \"Savings\""),
                Some(cash_id),
            )
            .await?;

        // Level 4: children of Staffing
        service
            .account_service
            .account_create(
                AccountId::from_str("ac5salrys0")?,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Salaries".to_string())
                    .expect("Failed to create a name from \"Salaries\""),
                Some(staffing_id),
            )
            .await?;
        service
            .account_service
            .account_create(
                AccountId::from_str("ac5bnfts00")?,
                maple_ridge_academy_id,
                pacioli_id,
                Name::try_new("Benefits".to_string())
                    .expect("Failed to create a name from \"Benefits\""),
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
        .transaction_service
        .transaction_get_all_in_journal(maple_ridge_academy_id, pacioli_id)
        .await?
        .is_empty()
    {
        // Transaction 1: Tuition deposited into checking - $5,000
        // Assets › Current Assets › Cash › Checking  /  Revenue
        service
            .transaction_service
            .transaction_create(
                TransactionId::from_str("t1tuition0000001")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        journal_id: maple_ridge_academy_id,
                        account_id: checking_id,
                        amount: 500000,
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        journal_id: maple_ridge_academy_id,
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
            .transaction_service
            .transaction_create(
                TransactionId::from_str("t2salary00000002")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        journal_id: maple_ridge_academy_id,
                        account_id: salaries_id,
                        amount: 320000,
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        journal_id: maple_ridge_academy_id,
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
            .transaction_service
            .transaction_create(
                TransactionId::from_str("t3textbooks00003")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        journal_id: maple_ridge_academy_id,
                        account_id: supplies_id,
                        amount: 85000,
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        journal_id: maple_ridge_academy_id,
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
            .transaction_service
            .transaction_create(
                TransactionId::from_str("t4tuition0000004")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        journal_id: maple_ridge_academy_id,
                        account_id: checking_id,
                        amount: 450000,
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        journal_id: maple_ridge_academy_id,
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
            .transaction_service
            .transaction_create(
                TransactionId::from_str("t5supplies000005")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        journal_id: maple_ridge_academy_id,
                        account_id: savings_id,
                        amount: 200000,
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        journal_id: maple_ridge_academy_id,
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
            .transaction_service
            .transaction_create(
                TransactionId::from_str("t6chkdeposit0006")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        journal_id: maple_ridge_academy_id,
                        account_id: benefits_id,
                        amount: 64000,
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        journal_id: maple_ridge_academy_id,
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
