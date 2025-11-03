#[cfg(feature = "ssr")]
#[allow(dead_code)]
pub(crate) mod api {
    use axum::Extension;
    use leptos::prelude::*;
    use leptos_axum::extract;
    use sqlx::PgPool;
    use std::collections::HashMap;
    use tower_sessions::Session;

    macro_rules! pull_database_and_client_info {
        ($x:ident, $y:ident, $z:ident) => {
            let Extension(session) = extract::<Extension<Session>>().await?;

            let Extension(pool) = extract::<Extension<PgPool>>().await?;

            // fofix the majority of errors in the app modulerce initialize the session
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

            let user_id = match sqlx::query_scalar::<_, Option<i32>>(
                "SELECT user_id FROM authenticated_sessions WHERE session_id = ?",
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

    #[server]
    pub async fn get_accounts() -> Result<Vec<(String, i64, bool)>, ServerFnError> {
        pull_database_and_client_info!(pool, _session_id, user_id);

        let mut accounts: Vec<(String, i64, bool)> = Vec::new();

        let account_ids: Vec<i32> =
            sqlx::query_scalar("SELECT id FROM account_connections WHERE user_id = ?")
                .bind(&user_id)
                .fetch_all(&pool)
                .await?;

        for id in account_ids {
            let account: (String, i64, bool) =
                sqlx::query_as("SELECT title, balance_cents, shared FROM accounts WHERE id = ?")
                    .bind(&id)
                    .fetch_one(&pool)
                    .await?;

            accounts.push((account.0, account.1, account.2));
        }

        Ok(accounts)
    }

    #[server]
    pub async fn add_account(title: String) -> Result<(), ServerFnError> {
        pull_database_and_client_info!(pool, _session_id, user_id);

        sqlx::query("INSERT INTO accounts (title, balance_cents) VALUES (?, 0)")
            .bind(&title.trim())
            .execute(&pool)
            .await?;

        let account_id =
            sqlx::query_scalar::<_, Option<i32>>("SELECT MAX(id) FROM accounts WHERE title = ?")
                .bind(&title.trim())
                .fetch_one(&pool)
                .await?
                .expect("the entry to exist");

        sqlx::query("INSERT INTO account_connections (id, user_id) VALUES (?,?)")
            .bind(&account_id)
            .bind(&user_id)
            .execute(&pool)
            .await?;

        Ok(())
    }

    #[server]
    pub async fn share_account(account_id: u32, username: String) -> Result<(), ServerFnError> {
        pull_database_and_client_info!(pool, _session_id, _user_id);

        let user_id: i32 = match sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
            .bind(&username)
            .fetch_one(&pool)
            .await
        {
            Ok(s) => s,
            Err(_) => {
                return Err(ServerFnError::ServerError(format!(
                    "A user with the username {} was not found!",
                    username
                )))
            }
        };

        sqlx::query("INSERT INTO account_connections (id, user_id) SELECT ?, ? WHERE NOT EXISTS(SELECT 1 FROM account_connections WHERE id = ? AND user_id = ?)")
        .bind(account_id as i32)
        .bind(&user_id)
        .bind(account_id as i32)
        .bind(&user_id)
        .execute(&pool)
        .await?;

        sqlx::query("UPDATE accounts SET shared = 1 WHERE id = ?")
            .bind(account_id as i32)
            .execute(&pool)
            .await?;

        Ok(())
    }

    #[server]
    pub async fn transact(
        acc_ids: Vec<String>,
        balance_add_cents: Vec<String>,
        balance_remove_cents: Vec<String>,
    ) -> Result<(), ServerFnError> {
        pull_database_and_client_info!(pool, _session_id, user_id);

        let mut balance_updates: Vec<(String, i64)> = Vec::new();

        for i in 0..acc_ids.len() {
            let account_add: i64 = balance_add_cents.get(i).unwrap().parse().unwrap_or(0);

            let account_remove: i64 = balance_remove_cents.get(i).unwrap().parse().unwrap_or(0);

            let account_change = account_add - account_remove;
            if account_change != 0 {
                balance_updates.push((acc_ids.get(i).unwrap().parse().unwrap(), account_change))
            }
        }

        let mut total_change = 0;

        for update in &balance_updates {
            total_change += update.1
        }

        if total_change != 0 {
            //return Ok(TransactionResult::BALANCEMISMATCH);
            // Any ok values are not being shown to the client for some reason. Will try to figure out later. Fixing for now by making the balancemismatch return an error
            return Err(ServerFnError::ServerError(
                "Your credits do not match your debits!".to_string(),
            ));
        }

        sqlx::query("INSERT INTO transactions (author_id) VALUES (?)")
            .bind(&user_id)
            .execute(&pool)
            .await?;

        // The table auto-increments the id, so I must fetch it so I know what to tag the children with.
        // binding the session ID prevents a race condition in the case where two users call transact() simultaneously
        let transaction_id = sqlx::query_scalar::<_, Option<i32>>(
            "SELECT MAX(id) FROM transactions WHERE author_id = ?",
        )
        .bind(&user_id)
        .fetch_one(&pool)
        .await?
        .expect("the entry to exist");

        for update in &balance_updates {
            if update.1 == 0 {
                continue;
            }

            sqlx::query(
           "INSERT INTO partial_transactions (id, account_id, balance_diff_cents) VALUES (?, ?, ?)"
       )
       .bind(&transaction_id)
       .bind(update.0.clone())
       .bind(update.1)
       .execute(&pool)
       .await?;

            sqlx::query("UPDATE accounts SET balance_cents = balance_cents + ? WHERE id = ?")
                .bind(update.1)
                .bind(update.0.clone())
                .bind(&user_id)
                .execute(&pool)
                .await?;
        }
        return Ok(());
    }

    pub async fn get_transaction_parents() -> Result<Vec<(i32, i64)>, ServerFnError> {
        pull_database_and_client_info!(pool, _session_id, user_id);

        let transactions: Vec<(i32, i64)> = match sqlx::query_as(
            "SELECT id, created_at FROM transactions WHERE author_id = ? ORDER BY created_at DESC",
        )
        .bind(user_id.unwrap() as i32)
        .fetch_all(&pool)
        .await
        {
            Ok(s) => s,
            Err(e) => return Err(ServerFnError::ServerError(e.to_string())),
        };

        Ok(transactions)
    }

    pub async fn get_transaction_children(
        transaction_id: i32,
    ) -> Result<Vec<(i32, i64)>, ServerFnError> {
        pull_database_and_client_info!(pool, _session_id, _user_id);

        let partial_transactions: Vec<(i32, i64)> = match sqlx::query_as(
        "SELECT account_id, balance_diff_cents FROM partial_transactions WHERE id = ? ORDER by balance_diff_cents ASC",
    )
    .bind(transaction_id as i32)
    .fetch_all(&pool)
    .await
    {
        Ok(s) => s,
        Err(e) => return Err(ServerFnError::ServerError(e.to_string())),
    };

        let mut account_name_map: HashMap<i32, String> = HashMap::new();

        for transaction in &partial_transactions {
            let name =
                sqlx::query_scalar::<_, Option<String>>("SELECT title FROM accounts WHERE id = ?")
                    .bind(transaction.0)
                    .fetch_one(&pool)
                    .await?
                    .expect("the entry to exist");

            account_name_map.insert(transaction.0, name);
        }

        Ok(partial_transactions)
    }

    pub async fn package_transactions() -> Result<Vec<((i32, i64), Vec<(i32, i64)>)>, ServerFnError>
    {
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
            let children = match get_transaction_children(parent.0).await {
                Ok(s) => s,
                Err(e) => {
                    return Err(ServerFnError::ServerError(format!(
                        "Unable to fetch transaction children: {} ",
                        e.to_string()
                    )))
                }
            };
            packaged_transactions.push((parent, children));
        }
        Ok(packaged_transactions)
    }

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
                "Your passwords do not match!".to_string(),
            ));
        }

        let Extension(session) = extract::<Extension<Session>>().await?;

        let Extension(pool) = extract::<Extension<PgPool>>().await?;

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

        let id: i32 =
            sqlx::query_scalar::<_, Option<i32>>("SELECT id FROM users WHERE username = ?")
                .bind(&username)
                .fetch_one(&pool)
                .await?
                .expect("the entry to exist");

        let session_id = match &session.id() {
            Some(s) => s.to_string(),
            None => {
                return Err(ServerFnError::ServerError(
                    "Session id not found, please refresh the page.".to_string(),
                ))
            }
        };

        sqlx::query("INSERT INTO authenticated_sessions (session_id, user_id) VALUES (?, ?)")
            .bind(&session_id)
            .bind(&id)
            .execute(&pool)
            .await?;

        Ok(())
    }

    #[server]
    pub async fn login(username: String, password: String) -> Result<(), ServerFnError> {
        let Extension(session) = extract::<Extension<Session>>().await?;

        let Extension(pool) = extract::<Extension<PgPool>>().await?;

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

        let account: (i32, String) =
            match sqlx::query_as("SELECT id, hash_and_salt  FROM users WHERE username = ?")
                .bind(&username)
                .fetch_one(&pool)
                .await
            {
                Ok(s) => s,
                Err(_) => {
                    return Err(ServerFnError::ServerError(format!(
                        "An account with the username \"{}\" does not exist. Please sign up.",
                        &username
                    )))
                }
            };

        if bcrypt::verify(password, &account.1).unwrap() {
            sqlx::query("INSERT INTO authenticated_sessions (session_id, user_id) VALUES (?, ?)")
                .bind(&session_id)
                .bind(account.0)
                .execute(&pool)
                .await?;
        } else {
            return Err(ServerFnError::ServerError(
                "Incorrect Password!".to_string(),
            ));
        }

        Ok(())
    }

    #[server]
    pub async fn is_logged_in() -> Result<(), ServerFnError> {
        pull_database_and_client_info!(_pool, _session_id, _user_id);
        // this will return an error if you are not logged in
        Ok(())
    }

    #[server]
    pub async fn log_out() -> Result<(), ServerFnError> {
        pull_database_and_client_info!(pool, session_id, _user_id);
        sqlx::query("DELETE FROM authenticated_sessions WHERE session_id = ?")
            .bind(&session_id)
            .execute(&pool)
            .await?;

        Ok(())
    }
}
