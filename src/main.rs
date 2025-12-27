#[allow(dead_code)]
mod site;

#[allow(dead_code)]
mod event_sourcing;

#[allow(dead_code)]
mod api;

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use dotenvy::dotenv;
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{LeptosRoutes, generate_route_list};
    use site::app::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::{Pool, Postgres};
    use std::env;
    use tower_sessions::{Expiry, SessionManagerLayer, cookie::time::Duration};
    use tower_sessions_sqlx_store::PostgresStore;

    let conf = get_configuration(None).expect("invalid configuration");
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    // Generate the list of routes in your Leptos App

    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("failed to get database url from .en");

    let pool: Pool<Postgres> = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("failed to connect to the postgres pool");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS user_events (
            id BIGSERIAL PRIMARY KEY,
            user_id UUID NOT NULL,
            event_type SMALLINT NOT NULL,
            payload BYTEA NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )",
    )
    .execute(&pool)
    .await
    .expect("failed to create the user events table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS journal_events (
            id BIGSERIAL PRIMARY KEY,
            journal_id UUID NOT NULL,
            event_type SMALLINT NOT NULL,
            payload BYTEA NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )",
    )
    .execute(&pool)
    .await
    .expect("failed to create the journal events table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS auth_events (
            id BIGSERIAL PRIMARY KEY,
            user_id UUID NOT NULL,
            session_id BYTEA NOT NULL,
            event_type SMALLINT NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )",
    )
    .execute(&pool)
    .await
    .expect("failed to create the auth events table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS username_events (
                id BIGSERIAL PRIMARY KEY,
                user_id UUID NOT NULL,
                username VARCHAR(64) NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )",
    )
    .execute(&pool)
    .await
    .expect("failed to create the auth events table");

    let session_store = PostgresStore::new(pool.clone());
    session_store
        .migrate()
        .await
        .expect("failed to migrate the session store");

    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(Duration::hours(48)));

    let routes = generate_route_list(App);

    let app = Router::new()
        .leptos_routes(&leptos_options, routes, {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(shell))
        .layer(axum::Extension(pool))
        .layer(session_layer)
        .with_state(leptos_options);

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    log!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind the tcp address");
    axum::serve(listener, app.into_make_service())
        .await
        .expect("failed to serve on the address");
}

#[cfg(not(feature = "ssr"))]
pub fn main() {
    // no client-side main function
    // unless we want this to work with e.g., Trunk for pure client-side testing
    // see lib.rs for hydration function instead
}
