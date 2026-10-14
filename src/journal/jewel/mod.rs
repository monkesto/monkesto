pub mod commands;
pub mod extract;
use crate::authn::get_user;
use crate::authority::{Actor, Authority};
use crate::dollars::Dollars;
use crate::journal::JournalId;
use crate::journal::error::JournalError;
use crate::journal::file::FileId;
use crate::journal::jewel::extract::JewelData;
use crate::journal::layout::layout;
use crate::{BackendType, StateType};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum_login::AuthSession;
use axum_login::tracing::error;
use chrono::{Datelike, NaiveDate, Utc};
use maud::{Markup, html};
use std::str::FromStr;
use thiserror::Error;

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
