mod api;
mod types;
use leptos::prelude::LeptosOptions;

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use prototype::app::*;
    use tower_sessions::SessionManagerLayer;
    use tower_sessions_sqlx_store::{sqlx::SqlitePool, SqliteStore};
    use types::*;

    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    // Generate the list of routes in your Leptos App

    let pool = SqlitePool::connect("sqlite:database.db")
        .await
        .expect("the database to exist");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS accounts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                title TEXT NOT NULL,
                balance_cents INTEGER NOT NULL DEFAULT 0
            )",
    )
    .execute(&pool)
    .await
    .expect("to be able to create a table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS transactions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id TEXT NOT NULL,
                created_at INTEGER DEFAULT (unixepoch())
            )",
    )
    .execute(&pool)
    .await
    .expect("to be able to create a table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS partial_transactions (
                id INTEGER NOT NULL,
                account_id TEXT NOT NULL,
                balance_diff_cents INTEGER NOT NULL
            )",
    )
    .execute(&pool)
    .await
    .expect("to be able to create a table");

    let session_store = SqliteStore::new(pool.clone());
    session_store
        .migrate()
        .await
        .expect("to be able to migrate the session store");

    let session_layer = SessionManagerLayer::new(session_store).with_secure(false);

    let routes = generate_route_list(App);

    let app = Router::new()
        .leptos_routes(&leptos_options, routes, App)
        .fallback(leptos_axum::file_and_error_handler(shell))
        .layer(axum::Extension(pool))
        .layer(session_layer)
        .with_state(leptos_options);

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    log!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for pure client-side testing
    // see lib.rs for hydration function instead
}
