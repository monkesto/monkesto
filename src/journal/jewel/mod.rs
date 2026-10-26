use std::io::Write;
pub mod commands;
pub mod extract;

use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::dollars::Dollars;
use crate::journal::account::{AccountId, AccountType};
use crate::journal::error::{JournalError, JournalResult};
use crate::journal::event::JournalDomainEvent;
use crate::journal::file::{FileId, ObjectStore};
use crate::journal::jewel::extract::{JewelData, jewel_extract};
use crate::journal::layout::layout;
use crate::journal::service::stream_lock::append_creation_event_in_tx;
use crate::journal::service::{EventId, JournalService};
use crate::journal::transaction::entry::{EntryId, EntryKind, EntrySide};
use crate::journal::transaction::memo::Memo;
use crate::journal::transaction::{FinancialPeriod, TransactionId};
use crate::journal::{JournalId, Permissions};
use crate::name::Name;
use crate::time::TimeProvider;
use crate::{BackendType, StateType};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_login::AuthSession;
use axum_login::tracing::error;
use chrono::{Datelike, NaiveDate, Utc};
use maud::{Markup, html};
use sqlx::ConnectOptions;
use sqlx::sqlite::SqliteConnectOptions;
use std::collections::HashMap;
use std::fs::File;
use std::io;
use std::process::Stdio;
use std::str::FromStr;
use std::time::Duration;
use tempfile::NamedTempFile;
use thiserror::Error;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

#[derive(Error, Debug, PartialEq)]
pub enum JewelImportError {
    #[error("failed to create a temp file or read bytes from a stream")]
    Io(String),
    #[error("failed to start a child process")]
    StartChildProcess(String),
    #[error("failed to write to a file or stdin of a child process")]
    Write(String),
    #[error("child process exited with an error: {0}")]
    ChildProcessExitFailure(String),
    #[error("the database uses an outdated version of jewel: {0}; jewel 9.0 or newer is required")]
    OutdatedJewelVersion(f64),
    #[error("missing entries for journal {0}")]
    MissingJournalEntries(i64),
    #[error("missing account with id {0}")]
    MissingAccount(i64),
}

impl IntoResponse for JewelImportError {
    fn into_response(self) -> Response {
        use JewelImportError::*;
        match self {
            Io(_) | StartChildProcess(_) | Write(_) | ChildProcessExitFailure(_) => {
                error!("{}", self);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
            OutdatedJewelVersion(_) => {
                (StatusCode::BAD_REQUEST, "jewel 9.0 or later is required").into_response()
            }

            MissingJournalEntries(_) => (
                StatusCode::BAD_REQUEST,
                "one or more journals is missing entries",
            )
                .into_response(),
            MissingAccount(_) => {
                (StatusCode::BAD_REQUEST, "one or more accounts is missing").into_response()
            }
        }
    }
}

// NOTE: Production implementations should call get_jewel_db as a secondary web request because it has to download a file from the internet and transform it with cli tools
// a latency of 500ms or more is expected from this endpoint
#[expect(clippy::let_unit_value)]
pub async fn view_db(
    State(state): State<StateType>,
    session: AuthSession<BackendType>,
    Path((journal_id, file_id)): Path<(String, String)>,
) -> Result<Markup, JournalError> {
    let user = get_user(session)?;
    let user_authority = Authority::Direct(Actor::User(user.id));
    let journal_id_res = JournalId::from_str(&journal_id);
    let file_id_res = FileId::from_str(&file_id);

    // Note(Ryan): history and number of top expenses can be configured here
    const NUM_MONTHS: usize = 36;
    const TOP_EXPENSE_LIST_SIZE: usize = 10;

    let markup = if let Ok(journal_id) = journal_id_res
        && let Ok(file_id) = file_id_res
    {
        html! {
            @match state.journal_service.get_jewel_db(journal_id, file_id, user_authority, NUM_MONTHS as u32, TOP_EXPENSE_LIST_SIZE).await {
                Ok(JewelData {standard_stats, accounts, ..}) => div {
                    @let current_date = Utc::now();

                    @let mut curr_year = current_date.year();
                    @let mut curr_month = current_date.month();


                    @for month in 0..NUM_MONTHS {
                        @let timestamp = NaiveDate::from_ymd_opt(curr_year, curr_month, 1).expect("valid date");

                        h2 class="text-4xl/7 font-bold text-white sm:truncate sm:text-3xl sm:tracking-tight" {
                            (timestamp.format("%B %C%y"))
                        }

                        @let budget_giving = standard_stats.monthly_budget_giving[month];

                        p {
                            "budget giving: " (Dollars(budget_giving))
                        }

                        @let tithe_giving = standard_stats.monthly_tithe[month];

                        p {
                            "tithe giving: " (Dollars(tithe_giving))
                        }

                        br;

                        @let (online_giving, inhouse_giving, unknown_giving) = standard_stats.monthly_online_vs_offline_giving[month];


                        p {
                            "received " (Dollars(online_giving)) " online"
                        }

                        p {
                            "received " (Dollars(inhouse_giving)) " in person"
                        }

                        p {
                            "received " (Dollars(unknown_giving)) " from an unknown source"
                        }

                        br;

                        @let budget_allocations = standard_stats.monthly_budget_allocations[month].clone();

                        table class="w-full text-left table-auto min-w-max" {
                            caption class="caption-top" {
                                "budget allocations"
                            }

                            thread {
                                tr {
                                    th {
                                        "Account"
                                    }
                                    th {
                                        "Month Spent"
                                    }
                                    th {
                                        "Month Budgeted"
                                    }
                                    th {
                                        "Month Difference"
                                    }
                                    th {
                                        "YTD Spent"
                                    }
                                    th {
                                        "YTD Budgeted"
                                    }
                                    th {
                                        "YTD Difference"
                                    }
                                }
                            }

                            tbody {
                                 @for (account_id, (month_budgeted, month_spent, year_budgeted, year_spent)) in budget_allocations {
                                    tr {
                                        td {
                                            (accounts.get(&account_id).map(|acc| acc.name.as_str()).unwrap_or("unknown account"))
                                        }
                                        td {
                                            (Dollars(month_spent))
                                        }
                                        td {
                                            (Dollars(month_budgeted))
                                        }

                                        @let month_difference = month_spent - month_budgeted;
                                        td {
                                            (Dollars(month_difference))
                                        }

                                        td {
                                            (Dollars(year_spent))
                                        }
                                        td {
                                            (Dollars(year_budgeted))
                                        }

                                        @let year_difference = year_spent - year_budgeted;
                                        td {
                                            (Dollars(year_difference))
                                        }
                                    }

                                }
                            }
                        }

                        br;

                        @let top_expenses = standard_stats.monthly_top_expenses[month].clone();

                        h3 class="text-2xl/7 font-bold text-white sm:truncate sm:text-xl sm:tracking-tight" {
                            "top " (TOP_EXPENSE_LIST_SIZE) " expenses"
                        }

                        ul {
                            @for (account_id, cost) in top_expenses {
                                (accounts.get(&account_id).map(|acc| acc.name.as_str()).unwrap_or("unknown account")) ": " (Dollars(cost))
                                br;
                            }
                        }

                        br;

                        @if curr_month > 1 {
                            @let _ = curr_month -= 1;
                        } @else {
                            @let _ = curr_year -= 1;
                            @let _ = curr_month = 12;
                        }
                    }
                },
                Err(e) => p {(e.to_string())}
            }
        }
    } else {
        html! {
            p {
                "invalid journal or file id"
            }
        }
    };

    Ok(layout(None, true, Some(&journal_id), markup))
}

impl JournalService {
    #[allow(clippy::too_many_arguments)]
    pub async fn import_jewel_database(
        &self,
        journal_id: JournalId,
        jewel_data: JewelData,
        authority: Authority,
    ) -> JournalResult<EventId> {
        let mut tx = self.pool.begin().await?;
        let mut event_id = 0;

        self.check_permissions_in_tx(
            &mut tx,
            journal_id,
            authority,
            Permissions::ADD_ACCOUNT
                | Permissions::CREATE_ACTIVITY
                | Permissions::CREATE_FUND
                | Permissions::APPEND_TRANSACTION,
        )
        .await?;

        let mut account_creation_timestamp = Utc::now();
        let mut account_id_map = HashMap::new();

        for (jewel_account_id, account) in jewel_data.accounts {
            if account.active {
                let account_type = if account.account_type < 3 {
                    AccountType::Asset
                } else {
                    AccountType::Liability
                };

                let account_id = AccountId::new();
                account_id_map.insert(jewel_account_id, account_id);

                // jewel doesn't record account creation timestamps, but its ids are sequential
                // since we're creating the accounts in that sequential order, we can keep the order
                // by creating each account 1 millisecond later than the last
                account_creation_timestamp += Duration::from_millis(1);

                event_id = append_creation_event_in_tx(
                    &mut tx,
                    JournalDomainEvent::AccountCreated {
                        journal_id,
                        account_id,
                        account_type,
                        name: Name::try_new(account.name.clone())?,
                    },
                    None,
                    authority,
                    account_creation_timestamp.get_time(),
                    None,
                )
                .await?;
            }
        }

        for (jewel_transaction_id, jewel_transaction) in jewel_data.journals {
            let transaction_id = TransactionId::new();
            let z_single_account_id = jewel_transaction.z_single_account_id;

            let mut entry_ids = Vec::new();
            let mut entry_sum = 0;

            for entry in jewel_data
                .journal_entries
                .get(&jewel_transaction_id)
                .ok_or(JewelImportError::MissingJournalEntries(
                    jewel_transaction_id,
                ))?
            {
                if entry.account_id == jewel_transaction.z_single_account_id {
                    continue;
                }

                let entry_amount = (entry.amount * 100.0).round() as i64;

                entry_sum += entry_amount;

                let entry_id = EntryId::new();
                entry_ids.push(entry_id);

                append_creation_event_in_tx(
                    &mut tx,
                    JournalDomainEvent::EntryCreated {
                        entry_id,
                        journal_id,
                        transaction_id,
                        amount: entry_amount as u64,
                        entry_side: if entry_amount > 0 {
                            EntrySide::Credit
                        } else {
                            EntrySide::Debit
                        },
                        entry_kind: EntryKind::Account {
                            account_id: *account_id_map
                                .get(&entry.account_id)
                                .ok_or(JewelImportError::MissingAccount(entry.account_id))?,
                        },
                    },
                    None,
                    authority,
                    jewel_transaction.accounting_date.get_time(),
                    None,
                )
                .await?;
            }

            let z_single_entry_id = EntryId::new();
            entry_ids.push(z_single_entry_id);

            append_creation_event_in_tx(
                &mut tx,
                JournalDomainEvent::EntryCreated {
                    entry_id: z_single_entry_id,
                    journal_id,
                    transaction_id,
                    amount: entry_sum.unsigned_abs(),
                    entry_side: if entry_sum > 0 {
                        EntrySide::Debit
                    } else {
                        EntrySide::Credit
                    },
                    entry_kind: EntryKind::Account {
                        account_id: *account_id_map
                            .get(&z_single_account_id)
                            .ok_or(JewelImportError::MissingAccount(z_single_account_id))?,
                    },
                },
                None,
                authority,
                jewel_transaction.accounting_date.get_time(),
                None,
            )
            .await?;

            event_id = append_creation_event_in_tx(
                &mut tx,
                JournalDomainEvent::TransactionCreated {
                    transaction_id,
                    journal_id,
                    financial_period: FinancialPeriod::from(jewel_transaction.accounting_date),
                    memo: Some(Memo::try_new(jewel_transaction.memo)?),
                },
                None,
                authority,
                jewel_transaction.accounting_date.get_time(),
                None,
            )
            .await?;
        }

        tx.commit().await?;
        Ok(event_id)
    }

    pub(crate) async fn get_jewel_db(
        &self,
        journal_id: JournalId,
        file_id: FileId,
        authority: Authority,
        history_length_months: u32,
        top_expense_list_size: usize,
    ) -> JournalResult<JewelData> {
        let file_key = self.get_file(file_id, journal_id, authority).await?.key();

        let (mdb_file, mdb_file_path) = match self.object_store() {
            ObjectStore::S3 {
                s3_client,
                bucket_name,
            } => {
                let mut response = s3_client
                    .get_object()
                    .bucket(bucket_name)
                    .key(file_key)
                    .send()
                    .await?;

                let mut mdb_file =
                    NamedTempFile::new().map_err(|e| JewelImportError::Io(e.to_string()))?;

                while let Some(bytes) = response
                    .body
                    .try_next()
                    .await
                    .map_err(|e| JewelImportError::Io(e.to_string()))?
                {
                    mdb_file
                        .write_all(bytes.as_ref())
                        .map_err(|e| JewelImportError::Write(e.to_string()))?;
                }

                mdb_file
                    .flush()
                    .map_err(|e| JewelImportError::Write(e.to_string()))?;

                let file_path = mdb_file.path().to_string_lossy().to_string();

                (mdb_file, file_path)
            }
            ObjectStore::Local { storage_directory } => {
                let stored_file_path = storage_directory.join(file_key);
                let mut stored_file = File::open(&stored_file_path)
                    .map_err(|e| JewelImportError::Io(e.to_string()))?;
                let mut mdb_file =
                    NamedTempFile::new().map_err(|e| JewelImportError::Io(e.to_string()))?;

                io::copy(&mut stored_file, &mut mdb_file)
                    .map_err(|e| JewelImportError::Io(e.to_string()))?;

                let file_path = mdb_file.path().to_string_lossy().to_string();

                (mdb_file, file_path)
            }
        };

        let sqlite_file = NamedTempFile::new().map_err(|e| JewelImportError::Io(e.to_string()))?;

        let sqlite_file_path = sqlite_file.path().to_string_lossy();

        let schema = Command::new("mdb-schema")
            .arg(&mdb_file_path)
            .arg("sqlite")
            .output()
            .await
            .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

        if !schema.status.success() {
            Err(JewelImportError::ChildProcessExitFailure(
                String::from_utf8_lossy(schema.stderr.as_slice()).to_string(),
            ))?;
        }

        let mut sqlite_process = Command::new("sqlite3")
            .arg(sqlite_file_path.as_ref())
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

        if let Some(mut stdin) = sqlite_process.stdin.take() {
            stdin
                .write_all(&schema.stdout)
                .await
                .map_err(|e| JewelImportError::Write(e.to_string()))?;
        }

        sqlite_process
            .wait()
            .await
            .map_err(|e| JewelImportError::Io(e.to_string()))?;

        let tables = Command::new("mdb-tables")
            .arg("-1")
            .arg(&mdb_file_path)
            .output()
            .await
            .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

        if !tables.status.success() {
            Err(JewelImportError::ChildProcessExitFailure(
                String::from_utf8_lossy(tables.stderr.as_slice()).to_string(),
            ))?;
        }

        let tables_str = String::from_utf8_lossy(&tables.stdout);

        for table in tables_str.lines() {
            let export_output = Command::new("mdb-export")
                .arg("-I")
                .arg("sqlite")
                .arg("-q")
                .arg("'")
                .arg(&mdb_file_path)
                .arg(table)
                .output()
                .await
                .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

            if !export_output.status.success() {
                Err(JewelImportError::ChildProcessExitFailure(
                    String::from_utf8_lossy(export_output.stderr.as_slice()).to_string(),
                ))?;
            }

            let mut insert_cmd = Command::new("sqlite3")
                .arg(sqlite_file_path.as_ref())
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| JewelImportError::StartChildProcess(e.to_string()))?;

            if let Some(mut stdin) = insert_cmd.stdin.take() {
                stdin
                    .write_all(&export_output.stdout)
                    .await
                    .map_err(|e| JewelImportError::Write(e.to_string()))?;
            }

            insert_cmd
                .wait()
                .await
                .map_err(|e| JewelImportError::Io(e.to_string()))?;
        }

        // we no longer need the mdb file
        drop(mdb_file);

        let mut conn = SqliteConnectOptions::new()
            .filename(sqlite_file_path.as_ref())
            .connect()
            .await?;

        // old versions of jewel stored the database version as an int
        #[allow(clippy::disallowed_methods)]
        let version: f64 = sqlx::query_scalar(
            r#"
                SELECT CAST(DBVersion AS REAL) as DBVersion FROM GeneralInfo;
            "#,
        )
        .fetch_one(&mut conn)
        .await?;

        if version < 9.0 {
            Err(JewelImportError::OutdatedJewelVersion(version))?;
        }

        Ok(jewel_extract(
            &mut conn,
            FinancialPeriod::July,
            history_length_months,
            top_expense_list_size,
        )
        .await?)
    }
}
