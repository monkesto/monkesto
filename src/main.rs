mod api;
mod types;
use leptos::prelude::LeptosOptions;

#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use std::path::Path;

    use axum::Router;
    use dotenvy::dotenv;
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use leptos_router::components::provide_server_redirect;
    use prototype::app::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::{Pool, Postgres};
    use std::env;
    use tower_sessions::{cookie::time::Duration, Expiry, SessionManagerLayer};
    use tower_sessions_sqlx_store::PostgresStore;

    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    // Generate the list of routes in your Leptos App

    dotenv().ok();

    let database_url = env::var("DATABASE_URL");

    let pool: Pool<Postgres> = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS events (
            id BIGSERIAL PRIMARY KEY,
            aggregate_id UUID NOT NULL,
            sequence_number INT NOT NULL,
            event_type INT NOT NULL,
            payload JSONB NOT NULL,
            created_at TIMESTAMPZ NOT NULL DEFAULT now()
            )",
    )
    .execute(&pool)
    .await
    .expect("to be able to create a table");

    let session_store = PostgresStore::new(pool.clone());
    session_store
        .migrate()
        .await
        .expect("to be able to migrate the session store");

    let session_layer = SessionManagerLayer::new(session_store)
        .with_expiry(Expiry::OnInactivity(Duration::hours(6)));

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
