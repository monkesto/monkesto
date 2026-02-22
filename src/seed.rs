use crate::auth::user::Email;
use crate::auth::user::UserStore;
use crate::authority::UserId;
use crate::ident::AccountId;
use crate::ident::JournalId;
use crate::ident::TransactionId;
use crate::journal::Permissions;
use crate::known_errors::KnownErrors;
use crate::service::Service;
use crate::transaction::BalanceUpdate;
use crate::transaction::EntryType;
use std::str::FromStr;

pub(crate) async fn seed_dev_data<S: Service>(service: &S) -> Result<(), KnownErrors> {
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
    }

    // again, the presence of any transactions should show that they were already seeded
    if service
        .transaction_get_all_in_journal(maple_ridge_academy_id, pacioli_id)
        .await?
        .is_empty()
    {
        service
            .transaction_create(
                TransactionId::from_str("t1tuition0000001")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: assets_id,
                        amount: 500000, // $5,000.00 in cents
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

        // Transaction 1: Tuition payment received - $5,000
        service
            .transaction_create(
                TransactionId::from_str("t1tuition0000001")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: assets_id,
                        amount: 500000, // $5,000.00 in cents
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

        // Transaction 2: Teacher salary payment - $3,200
        service
            .transaction_create(
                TransactionId::from_str("t2salary00000002")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: expenses_id,
                        amount: 320000, // $3,200.00
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: assets_id,
                        amount: 320000,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;

        // Transaction 3: Textbook purchase - $850
        service
            .transaction_create(
                TransactionId::from_str("t3textbooks00003")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: expenses_id,
                        amount: 85000, // $850.00
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: assets_id,
                        amount: 85000,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;

        // Transaction 4: Another tuition payment - $4,500
        service
            .transaction_create(
                TransactionId::from_str("t3textbooks00003")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: assets_id,
                        amount: 450000, // $4,500.00
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

        // Transaction 5: Supplies purchase - $425
        service
            .transaction_create(
                TransactionId::from_str("t5supplies000005")?,
                maple_ridge_academy_id,
                pacioli_id,
                vec![
                    BalanceUpdate {
                        account_id: expenses_id,
                        amount: 42500, // $425.00
                        entry_type: EntryType::Debit,
                    },
                    BalanceUpdate {
                        account_id: assets_id,
                        amount: 42500,
                        entry_type: EntryType::Credit,
                    },
                ],
            )
            .await?;
    }

    Ok(())
}
