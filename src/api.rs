macro_rules! pull_database_and_client_info {
    ($x:ident, $y:ident, $z:ident) => {
        let Extension(session) = extract::<Extension<Session>>().await?;

        let Extension(pool) = extract::<Extension<SqlitePool>>().await?;

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

        let session_id = match session.id() {
            Some(s) => s.to_string(),
            None => {
                return Err(ServerFnError::ServerError(
                    "Session id not found-try refreshing the page".to_string(),
                ))
            }
        };

        let user_id = match sqlx::query_scalar::<_, Option<u32>>(
            "SELECT 1 user_id FROM authenticated_sessions WHERE session_id = ?",
        )
        .bind(&session_id)
        .fetch_one(&pool)
        .await
        {
            Ok(s) => s,
            Err(_) => {
                return Err(ServerFnError::ServerError(
                    "You must be logged in!".to_string(),
                ));
            }
        };

        let $x = pool;
        let $y = session_id;
        let $z = user_id;
    };
}

#[cfg(feature = "ssr")]
use std::collections::HashMap;

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
use crate::types::{
    Account, BalanceUpdate, PackagedTransaction, PartialTransaction, Transaction, TransactionResult,
};

#[cfg(feature = "ssr")]
#[server]
pub async fn get_accounts() -> Result<Vec<Account>, ServerFnError> {
    pull_database_and_client_info!(pool, _session_id, user_id);

    let accounts: Vec<(u32, String, i64)> = sqlx::query_as(
        "SELECT id, title, balance_cents FROM accounts WHERE user_id = ? ORDER BY id ASC",
    )
    .bind(&user_id)
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
    pull_database_and_client_info!(pool, _session_id, user_id);

    sqlx::query("INSERT INTO accounts (user_id, title, balance_cents) VALUES (?, ?, 0)")
        .bind(&user_id)
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
    pull_database_and_client_info!(pool, _session_id, user_id);

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

    sqlx::query("INSERT INTO transactions (user_id) VALUES (?)")
        .bind(&user_id)
        .execute(&pool)
        .await?;

    // The table auto-increments the id, so I must fetch it so I know what to tag the children with.
    // binding the session ID prevents a race condition in the case where two users call transact() simultaneously
    let transaction_id = sqlx::query_scalar::<_, Option<u32>>(
        "SELECT MAX(id) FROM transactions WHERE user_id = ?",
    )
    .bind(&user_id)
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
            "UPDATE accounts SET balance_cents = balance_cents + ? WHERE id = ? AND user_id = ?",
        )
        .bind(update.balance_diff_cents)
        .bind(update.id)
        .bind(&user_id)
        .execute(&pool)
        .await?;
    }
    return Ok(TransactionResult::UPDATED);
}

#[cfg(feature = "ssr")]
pub async fn get_transaction_parents() -> Result<Vec<Transaction>, ServerFnError> {
    pull_database_and_client_info!(pool, _session_id, user_id);

    let transactions: Vec<(u32, i64)> = match sqlx::query_as(
        "SELECT id, created_at FROM transactions WHERE user_id = ? ORDER BY created_at DESC",
    )
    .bind(&user_id)
    .fetch_all(&pool)
    .await
    {
        Ok(s) => s,
        Err(e) => return Err(ServerFnError::ServerError(e.to_string())),
    };

    Ok(transactions
        .into_iter()
        .map(|(id, created_at)| Transaction { id, created_at })
        .collect())
}

#[cfg(feature = "ssr")]
pub async fn get_transaction_children(
    transaction_id: u32,
) -> Result<Vec<PartialTransaction>, ServerFnError> {
    use crate::types::PartialTransaction;

    pull_database_and_client_info!(pool, _session_id, user_id);

    let partial_transactions: Vec<(u32, i64)> = match sqlx::query_as(
        "SELECT account_id, balance_diff_cents FROM partial_transactions WHERE id = ? ORDER by balance_diff_cents ASC",
    )
    .bind(&transaction_id)
    .fetch_all(&pool)
    .await
    {
        Ok(s) => s,
        Err(e) => return Err(ServerFnError::ServerError(e.to_string())),
    };

    let mut account_name_map: HashMap<u32, String> = HashMap::new();

    for transaction in &partial_transactions {
        let name = sqlx::query_scalar::<_, Option<String>>(
            "SELECT title FROM accounts WHERE user_id = ? AND id = ?",
        )
        .bind(&user_id)
        .bind(transaction.0)
        .fetch_one(&pool)
        .await?
        .expect("the entry to exist");

        account_name_map.insert(transaction.0, name);
    }

    Ok(partial_transactions
        .into_iter()
        .map(|(account_id, balance_diff_cents)| PartialTransaction {
            transaction_id: transaction_id,
            account_id,
            account_name: account_name_map.get(&account_id).unwrap().to_string(),
            balance_diff_cents,
        })
        .collect())
}

#[cfg(feature = "ssr")]
pub async fn package_transactions() -> Result<Vec<PackagedTransaction>, ServerFnError> {
    let mut packaged_transactions = Vec::new();

    let transaction_parents = match get_transaction_parents().await {
        Ok(s) => s,
        Err(e) => {
            return Err(ServerFnError::ServerError(format!(
                "failed to fetch transaction parents: {}",
                e.to_string()
            )))
        }
    };

    for parent in transaction_parents {
        let children = match get_transaction_children(parent.id).await {
            Ok(s) => s,
            Err(e) => {
                return Err(ServerFnError::ServerError(format!(
                    "Unable to fetch transaction children: {} ",
                    e.to_string()
                )))
            }
        };
        packaged_transactions.push(PackagedTransaction { parent, children });
    }
    Ok(packaged_transactions)
}

#[cfg(feature = "ssr")]
#[server]
pub async fn create_account(
    username: String,
    password: String,
    confirm_password: String,
) -> Result<(), ServerFnError> {
    use bcrypt::DEFAULT_COST;
    use rand::{rngs::OsRng, TryRngCore};

    if password != confirm_password {
        return Err(ServerFnError::ServerError(
            ("Your passwords do not match!".to_string()),
        ));
    }

    let Extension(session) = extract::<Extension<Session>>().await?;

    let Extension(pool) = extract::<Extension<SqlitePool>>().await?;

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

    let account_exists: (i64,) =
        sqlx::query_as("SELECT EXISTS(SELECT 1 FROM users WHERE username = ?)")
            .bind(&username)
            .fetch_one(&pool)
            .await?;

    if account_exists.0 == 1 {
        return Err(ServerFnError::ServerError(format!(
            "An account with the username {} already exists!",
            username
        )));
    }

    let mut rng = OsRng;
    let mut salt = [0u8; 16];

   let _ = rng.try_fill_bytes(&mut salt);

    sqlx::query("INSERT INTO users (username, hash_and_salt) VALUES (?, ?)")
        .bind(&username)
        .bind(bcrypt::hash(password, DEFAULT_COST).unwrap())
        .execute(&pool)
        .await?;

    Ok(())
}

#[cfg(feature = "ssr")]
#[server]
pub async fn login(username: String, password: String) -> Result<(), ServerFnError> {
    let Extension(session) = extract::<Extension<Session>>().await?;

    let Extension(pool) = extract::<Extension<SqlitePool>>().await?;

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

    let session_id = match session.id() {
        Some(s) => s.to_string(),
        None => {
            return Err(ServerFnError::ServerError(
                "Session id not found-try refreshing the page".to_string(),
            ))
        }
    };

    let account: (u32, String) = match sqlx::query_as("SELECT id, hash_and_salt  FROM users WHERE username = ?")
        .bind(&username)
        .fetch_one(&pool)
        .await {
          Ok(s) => s,
          Err(_) => return Err(ServerFnError::ServerError(format!("An account with the username \"{}\" does not exist. Please sign up.", &username))),
        };

    if bcrypt::verify(password, &account.1).unwrap() {
        sqlx::query("INSERT INTO authenticated_sessions (session_id, user_id) VALUES (?, ?)")
            .bind(&session_id)
            .bind(account.0)
            .execute(&pool)
            .await?;
    } else {
        return Err(ServerFnError::ServerError("Incorrect Password!".to_string()));
    }

    Ok(())
}

#[cfg(feature = "ssr")]
#[server]
pub async fn is_logged_in() -> Result<(), ServerFnError> {
    pull_database_and_client_info!(_pool, _session_id, _user_id);
    // this will return an error if you are not logged in
    Ok(())
}

#[cfg(feature = "ssr")]
#[server]
pub async fn log_out() -> Result<(), ServerFnError> {
    pull_database_and_client_info!(pool, session_id, _user_id);
    sqlx::query("DELETE FROM authenticated_sessions WHERE session_id = ?")
        .bind(&session_id)
        .execute(&pool)
        .await?;

    Ok(())
}
