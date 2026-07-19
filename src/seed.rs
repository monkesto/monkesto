use crate::AppState;
use crate::authn::user::{DEV_USERS, UserError};
use crate::authority::Actor;
use crate::authority::Authority;
use crate::authority::UserId;
use crate::journal::account::{AccountError, AccountId};
use crate::journal::transaction::EntryType;
use crate::journal::transaction::{BalanceUpdate, TransactionError, TransactionId};
use crate::journal::{JournalError, JournalId, Permissions};
use crate::monkesto_error::MonkestoResult;
use crate::name::Name;
use crate::time_provider::{IncrementalTimeProvider, TimeProvider};
use disintegrate::DecisionError;
use std::str::FromStr;

pub(crate) async fn seed_dev_data(state: &AppState) -> MonkestoResult<()> {
    let time_provider = IncrementalTimeProvider::new();

    let mut latest_user_event = 0;

    for (email, (user_id, webauthn_uuid)) in DEV_USERS.clone() {
        match state
            .authn_service
            .create_user(
                user_id,
                email.clone(),
                webauthn_uuid,
                Authority::Direct(Actor::System),
                time_provider.get_time(),
            )
            .await
        {
            Ok(ev_id) => latest_user_event = ev_id,

            // the user was already seeded
            Err(DecisionError::Domain(UserError::IdConflict(_))) => {}

            Err(_) => return Err(UserError::SeedFailure(email))?,
        }
    }

    let pacioli_id = UserId::from_str("zk8m3p5q7r2n4v6x")?;
    let wedgwood_id = UserId::from_str("yj7l2o4p6q8s0u1w")?;

    let pacioli_authority = Authority::Direct(Actor::User(pacioli_id));

    let maple_ridge_academy_id = JournalId::from_str("ab1cd2ef3g")?;
    let smith_and_sons_id = JournalId::from_str("hi4jk5lm6n")?;
    let green_valley_id = JournalId::from_str("op7qr8st9u")?;

    let assets_id = AccountId::from_str("ac1assets0")?;
    let revenue_id = AccountId::from_str("ac4revenue")?;
    let expenses_id = AccountId::from_str("ac5expense")?;

    let mut latest_journal_event = 0;

    let journals = [
        (
            maple_ridge_academy_id,
            Name::try_new("Maple Ridge Academy".to_string())?,
        ),
        (
            smith_and_sons_id,
            Name::try_new("Smith & Sons Bakery".to_string())?,
        ),
        (
            green_valley_id,
            Name::try_new("Green Valley Farm Co.".to_string())?,
        ),
    ];

    for (id, name) in journals {
        match state
            .journal_service
            .create_journal(
                id,
                pacioli_id,
                name,
                pacioli_authority.clone(),
                time_provider.get_time(),
            )
            .await
        {
            Ok(ev_id) => latest_journal_event = ev_id,
            // journal already exists, ignore
            Err(DecisionError::Domain(JournalError::IdCollision(_))) => {}
            Err(e) => return Err(e.into()),
        }
    }

    match state
        .journal_service
        .add_member(
            maple_ridge_academy_id,
            wedgwood_id,
            Permissions::READ | Permissions::ADD_ACCOUNT | Permissions::APPEND_TRANSACTION,
            pacioli_authority.clone(),
            time_provider.get_time(),
        )
        .await
    {
        Ok(ev_id) => latest_journal_event = ev_id,
        Err(DecisionError::Domain(JournalError::UserAlreadyHasAccess(_))) => {}
        Err(e) => return Err(e.into()),
    }

    let accounts = [
        (assets_id, Name::try_new("Assets".to_string())?),
        (
            AccountId::from_str("ac2liabili")?,
            Name::try_new("Liabilities".to_string())?,
        ),
        (
            AccountId::from_str("ac3equity0")?,
            Name::try_new("Equity".to_string())?,
        ),
        (revenue_id, Name::try_new("Revenue".to_string())?),
        (expenses_id, Name::try_new("Expenses".to_string())?),
    ];

    for (id, name) in accounts {
        match state
            .journal_service
            .create_account(
                id,
                maple_ridge_academy_id,
                name,
                pacioli_authority.clone(),
                time_provider.get_time(),
            )
            .await
        {
            Ok(ev_id) => latest_journal_event = ev_id,
            Err(DecisionError::Domain(AccountError::IdCollision(_))) => {}
            Err(e) => return Err(e.into()),
        }
    }

    let transactions = [
        (
            TransactionId::from_str("t1tuition0000001")?,
            vec![
                BalanceUpdate {
                    account_id: assets_id,
                    amount: 500000,
                    entry_type: EntryType::Debit,
                },
                BalanceUpdate {
                    account_id: revenue_id,
                    amount: 500000,
                    entry_type: EntryType::Credit,
                },
            ],
        ),
        (
            TransactionId::from_str("t2salary00000002")?,
            vec![
                BalanceUpdate {
                    account_id: expenses_id,
                    amount: 320000,
                    entry_type: EntryType::Debit,
                },
                BalanceUpdate {
                    account_id: assets_id,
                    amount: 320000,
                    entry_type: EntryType::Credit,
                },
            ],
        ),
        (
            TransactionId::from_str("t3textbooks00003")?,
            vec![
                BalanceUpdate {
                    account_id: expenses_id,
                    amount: 85000,
                    entry_type: EntryType::Debit,
                },
                BalanceUpdate {
                    account_id: assets_id,
                    amount: 85000,
                    entry_type: EntryType::Credit,
                },
            ],
        ),
        (
            TransactionId::from_str("t4tuition0000004")?,
            vec![
                BalanceUpdate {
                    account_id: assets_id,
                    amount: 450000,
                    entry_type: EntryType::Debit,
                },
                BalanceUpdate {
                    account_id: revenue_id,
                    amount: 450000,
                    entry_type: EntryType::Credit,
                },
            ],
        ),
        (
            TransactionId::from_str("t6chkdeposit0005")?,
            vec![
                BalanceUpdate {
                    account_id: expenses_id,
                    amount: 64000,
                    entry_type: EntryType::Debit,
                },
                BalanceUpdate {
                    account_id: assets_id,
                    amount: 64000,
                    entry_type: EntryType::Credit,
                },
            ],
        ),
    ];

    for (id, entries) in transactions {
        match state
            .journal_service
            .create_transaction(
                id,
                maple_ridge_academy_id,
                entries,
                pacioli_authority.clone(),
                time_provider.get_time(),
            )
            .await
        {
            Ok(ev_id) => latest_journal_event = ev_id,
            Err(DecisionError::Domain(TransactionError::IdCollision(_))) => {}
            Err(e) => return Err(e.into()),
        }
    }

    // wait for the projections to update
    state.authn_service.wait_for(latest_user_event).await;
    state.journal_service.wait_for(latest_journal_event).await;

    Ok(())
}
