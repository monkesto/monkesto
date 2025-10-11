macro_rules! pull_database_and_client_info {
    ($x:ident, $y:ident) => {
        let Extension(session) = extract::<Extension<Session>>().await?;

        let Extension($x) = extract::<Extension<SqlitePool>>().await?;

        // force initialize the session
        if session
            .get::<bool>("initialized")
            .await
            .ok()
            .flatten()
            .is_none()
        {
            match session.insert("initialized", true).await {
                Ok(s) => s,
                Err(e) => return Err(ServerFnError::ServerError(e.to_string())),
            };
        }

        let $y = match session.id() {
            Some(s) => s.to_string(),
            None => {
                return Err(ServerFnError::ServerError(
                    "Session id not found-try refreshing the page".to_string(),
                ))
            }
        };
    };
}

#[cfg(feature = "ssr")]
use axum::Extension;

use leptos::prelude::*;

#[cfg(feature = "ssr")]
use sqlx::SqlitePool;

#[cfg(feature = "ssr")]
use leptos_axum::extract;

#[cfg(feature = "ssr")]
use tower_sessions::Session;

#[cfg(feature = "ssr")]
use crate::types::{Account, BalanceUpdate, TransactionResult};

#[cfg(feature = "ssr")]
#[server]
pub async fn get_accounts() -> Result<Vec<Account>, ServerFnError> {
    pull_database_and_client_info!(pool, session_id);

    let accounts: Vec<(u32, String, i64)> = sqlx::query_as(
        "SELECT id, title, balance_cents FROM accounts WHERE session_id = ? ORDER BY id ASC",
    )
    .bind(&session_id)
    .fetch_all(&pool)
    .await?;

    Ok(accounts
        .into_iter()
        .map(|(id, title, balance_cents)| Account {
            id,
            title,
            balance_cents,
        })
        .collect())
}

#[cfg(feature = "ssr")]
#[server]
pub async fn add_account(title: String) -> Result<(), ServerFnError> {
    pull_database_and_client_info!(pool, session_id);

    sqlx::query("INSERT INTO accounts (session_id, title, balance_cents) VALUES (?, ?, 0)")
        .bind(&session_id)
        .bind(title.trim())
        .execute(&pool)
        .await?;

    Ok(())
}

#[cfg(feature = "ssr")]
#[server]
pub async fn transact(
    id_one: u32,
    balance_add_cents_one: i64,
    balance_remove_cents_one: i64,
    id_two: u32,
    balance_add_cents_two: i64,
    balance_remove_cents_two: i64,
    id_three: u32,
    balance_add_cents_three: i64,
    balance_remove_cents_three: i64,
) -> Result<TransactionResult, ServerFnError> {
    pull_database_and_client_info!(pool, session_id);

    let balance_updates = [
        BalanceUpdate {
            id: id_one,
            balance_diff_cents: balance_add_cents_one,
        },
        BalanceUpdate {
            id: id_one,
            balance_diff_cents: -balance_remove_cents_one,
        },
        BalanceUpdate {
            id: id_two,
            balance_diff_cents: balance_add_cents_two,
        },
        BalanceUpdate {
            id: id_two,
            balance_diff_cents: -balance_remove_cents_two,
        },
        BalanceUpdate {
            id: id_three,
            balance_diff_cents: balance_add_cents_three,
        },
        BalanceUpdate {
            id: id_three,
            balance_diff_cents: -balance_remove_cents_three,
        },
    ];

    let mut total_change = 0;

    for update in &balance_updates {
        total_change += update.balance_diff_cents
    }

    if total_change != 0 {
        //return Ok(TransactionResult::BALANCEMISMATCH);
        // Any ok values are not being shown to the client for some reason. Will try to figure out later. Fixing for now by making the balancemismatch return an error
        return Err(ServerFnError::ServerError(
            "Your credits do not match your debits!".to_string(),
        ));
    }

    sqlx::query("INSERT INTO transactions (session_id) VALUES (?)")
        .bind(&session_id)
        .execute(&pool)
        .await?;

    // The table auto-increments the id, so I must fetch it so I know what to tag the children with.
    // binding the session ID prevents a race condition in the case where two users call transact() simultaneously
    let transaction_id = sqlx::query_scalar::<_, Option<u32>>(
        "SELECT MAX(id) FROM transactions WHERE session_id = ?",
    )
    .bind(&session_id)
    .fetch_one(&pool)
    .await?
    .expect("the entry to exist");

    for update in &balance_updates {
        if update.balance_diff_cents == 0 {
            continue;
        }
        sqlx::query(
           "INSERT INTO partial_transactions (id, account_id, balance_diff_cents) VALUES (?, ?, ?)"
       )
       .bind(&transaction_id)
       .bind(update.id)
       .bind(update.balance_diff_cents)
       .execute(&pool)
       .await?;

        sqlx::query(
            "UPDATE accounts SET balance_cents = balance_cents + ? WHERE id = ? AND session_id = ?",
        )
        .bind(update.balance_diff_cents)
        .bind(update.id)
        .bind(&session_id)
        .execute(&pool)
        .await?;
    }
    return Ok(TransactionResult::UPDATED);
}
