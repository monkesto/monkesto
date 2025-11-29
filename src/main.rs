#[allow(dead_code)]
mod app;

#[allow(dead_code)]
mod event_sourcing;

#[allow(dead_code)]
mod main_api;

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use app::*;
    use axum::Router;
    use dotenvy::dotenv;
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{LeptosRoutes, generate_route_list};
    use sqlx::postgres::PgPoolOptions;
    use sqlx::{Pool, Postgres};
    use std::env;
    use tower_sessions::{Expiry, SessionManagerLayer, cookie::time::Duration};
    use tower_sessions_sqlx_store::PostgresStore;

    let conf = get_configuration(None).expect("invaid configuration");
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    // Generate the list of routes in your Leptos App

    dotenv().ok();

    let user = env::var("POSTGRES_USER").expect("failed to get postgres user from .env");

    let password =
        env::var("POSTGRES_PASSWORD").expect("failed to get postgres password from .env");

    let host = env::var("DATABASE_HOST").expect("failed to get database host from .env");

    let db = env::var("POSTGRES_DB").expect("failed to get postgres database from .env");

    let pool: Pool<Postgres> = PgPoolOptions::new()
        .max_connections(5)
        .connect(&format!(
            "postgres://{}:{}@{}:5432/{}",
            user, password, host, db
        ))
        .await
        .expect("failed to connect to the postgres pool");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS events (
            id BIGSERIAL PRIMARY KEY,
            aggregate_id UUID NOT NULL,
            aggregate_type SMALLINT NOT NULL,
            event_type SMALLINT NOT NULL,
            payload JSONB NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )",
    )
    .execute(&pool)
    .await
    .expect("failed to create the events table");

    let session_store = PostgresStore::new(pool.clone());
    session_store
        .migrate()
        .await
        .expect("failed to migrate the session store");

    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(Duration::hours(48)));

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
