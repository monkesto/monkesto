use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::error::MonkestoError;
use crate::error::monkesto_error::OrRedirect;
use crate::journal::JournalId;
use crate::journal::account::{AccountId, AccountType};
use crate::journal::entry::{EntryKind, EntrySide};
use crate::journal::error::JournalError;
use crate::journal::file::FileId;
use crate::journal::jewel::JewelImportError;
use crate::journal::transaction::memo::Memo;
use crate::journal::transaction::{FinancialPeriod, TransactionEntry, TransactionId};
use crate::name::Name;
use crate::time::TimeProvider;
use crate::{BackendType, StateType};
use axum::extract::{Path, State};
use axum::response::Redirect;
use axum_login::AuthSession;
use axum_login::tracing::info;
use chrono::Utc;
use disintegrate::DecisionError;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::{Duration, Instant};

pub async fn import_jewel_db(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((journal_id, file_id)): Path<(String, String)>,
) -> Result<Redirect, Redirect> {
    let callback_url = &format!("/journal/{}/file", journal_id);

    let user = get_user(session)?;
    let user_authority = Authority::Direct(Actor::User(user.id));
    let journal_id = JournalId::from_str(&journal_id).or_redirect(callback_url)?;
    let file_id = FileId::from_str(&file_id).or_redirect(callback_url)?;

    let mut jewel_account_id_map = HashMap::new();

    let data_gather_start = Instant::now();

    let jewel_data = state
        .journal_service
        .get_jewel_db(journal_id, file_id, user_authority, 0, 0)
        .await
        .or_redirect(callback_url)?;

    info!(
        "Jewel database imported in {:?})",
        data_gather_start.elapsed()
    );

    let mut creation_timestamp = Utc::now();

    let account_import_start = Instant::now();
    let mut num_imported_accounts = 0;

    for (jewel_account_id, account) in jewel_data.accounts {
        if account.active {
            let account_type = if account.account_type < 3 {
                AccountType::Asset
            } else {
                AccountType::Liability
            };

            // jewel doesn't record account creation timestamps, but its ids are sequential
            // since we're creating the accounts in that sequential order, we can keep the order
            // by creating each account 1 millisecond later than the last
            creation_timestamp += Duration::from_millis(1);

            // looping pattern to retry in the case of an id collision
            let mut account_id = AccountId::new();
            num_imported_accounts += 1;
            loop {
                match state
                    .journal_service
                    .create_account(
                        account_id,
                        journal_id,
                        // jewel names appear to be max 50 characters, ours are max 64
                        Name::try_new(account.name.clone()).or_redirect(callback_url)?,
                        account_type,
                        user_authority,
                        creation_timestamp.get_time(),
                    )
                    .await
                {
                    Ok(_) => break,
                    Err(DecisionError::Domain(JournalError::AccountIdCollision(_))) => {
                        account_id = AccountId::new()
                    }
                    Err(e) => Err(MonkestoError::from(e).redirect(callback_url))?,
                };
            }
            jewel_account_id_map.insert(jewel_account_id, account_id);
        }
    }

    info!(
        "Imported {} accounts in {:?} (total {:?})",
        num_imported_accounts,
        account_import_start.elapsed(),
        data_gather_start.elapsed()
    );

    let start_transaction_import = Instant::now();
    let mut rotating_transaction_import = Instant::now();

    let mut num_transactions = 0;

    for (jewel_transaction_id, jewel_transaction) in jewel_data.journals {
        let z_single_account_id = jewel_transaction.z_single_account_id;

        let mut entries = Vec::new();

        let mut entry_sum = 0;

        for entry in jewel_data
            .journal_entries
            .get(&jewel_transaction_id)
            .ok_or(JewelImportError::MissingJournalEntries(
                jewel_transaction_id,
            ))
            .map_err(JournalError::JewelImport)
            .or_redirect(callback_url)?
        {
            if entry.account_id == jewel_transaction.z_single_account_id {
                continue;
            }

            let entry_amount = (entry.amount * 100.0).round() as i64;

            entry_sum += entry_amount;

            entries.push(TransactionEntry {
                amount: entry_amount.unsigned_abs(),
                entry_side: if entry_amount > 0 {
                    EntrySide::Debit
                } else {
                    EntrySide::Credit
                },
                entry_kind: EntryKind::Account {
                    account_id: *jewel_account_id_map
                        .get(&entry.account_id)
                        .ok_or(JewelImportError::MissingAccount(entry.account_id))
                        .map_err(JournalError::JewelImport)
                        .or_redirect(callback_url)?,
                },
            })
        }

        entries.push(TransactionEntry {
            amount: entry_sum.unsigned_abs(),
            entry_side: if entry_sum < 0 {
                EntrySide::Debit
            } else {
                EntrySide::Credit
            },
            entry_kind: EntryKind::Account {
                account_id: *jewel_account_id_map
                    .get(&z_single_account_id)
                    .ok_or(JewelImportError::MissingAccount(z_single_account_id))
                    .map_err(JournalError::JewelImport)
                    .or_redirect(callback_url)?,
            },
        });

        let mut transaction_id = TransactionId::new();

        loop {
            match state
                .journal_service
                .create_transaction(
                    transaction_id,
                    journal_id,
                    // cloning feels really silly here, but making this borrowed is actually quite challenging
                    // the performance impact should be negligible because the list should be small and
                    // the chances of an id collision are mathematically insignificant
                    entries.clone(),
                    FinancialPeriod::from(jewel_transaction.accounting_date),
                    // jewel memos appear to be max 50 characters, ours are max 100 characters
                    Some(
                        Memo::try_new(jewel_transaction.memo.clone())
                            .map_err(JournalError::Memo)
                            .or_redirect(callback_url)?,
                    ),
                    user_authority,
                    jewel_transaction.accounting_date.get_time(),
                )
                .await
            {
                Ok(_) => break,
                Err(DecisionError::Domain(JournalError::TransactionIdCollision(_))) => {
                    transaction_id = TransactionId::new()
                }
                Err(e) => Err(MonkestoError::from(e).redirect(callback_url))?,
            };
        }

        num_transactions += 1;
        if num_transactions % 1000 == 0 {
            info!(
                "Imported {} transactions in {:?} (last 1000: {:?}), total elapsed: {:?}",
                num_transactions,
                start_transaction_import.elapsed(),
                rotating_transaction_import.elapsed(),
                data_gather_start.elapsed()
            );
            rotating_transaction_import = Instant::now();
        }
    }

    info!("Final jewel import time: {:?}", data_gather_start.elapsed());

    Ok(Redirect::to(&format!("/journal/{}", journal_id)))
}
