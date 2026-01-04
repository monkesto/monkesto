mod auth;
mod cuid;
mod journal;
mod known_errors;
mod maud_header;
mod notfoundpage;
mod webauthn;

use axum::Router;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::response::Redirect;
use axum::routing::get;
use axum::routing::post;
use axum_login::{AuthManagerLayerBuilder, login_required};
use dotenvy::dotenv;
use sqlx::postgres::PgPoolOptions;
use sqlx::{Pool, Postgres};
use std::env;
use tower_http::services::ServeFile;
use tower_sessions::SessionManagerLayer;
use tower_sessions_sqlx_store::PostgresStore;

use crate::auth::axum_login::Backend;

// Allow using tracing macros anywhere without needing to import them
#[macro_use]
extern crate tracing;

#[tokio::main]
async fn main() {
    dotenv().ok();

    if env::var("RUST_LOG").is_err() {
        unsafe {
            // Concurrent writing of set_var is not permitted,
            // but we're in main, so that shouldn't be a problem.
            env::set_var("RUST_LOG", "INFO");
        }
    }
    tracing_subscriber::fmt::init();

    let addr = env::var("SITE_ADDR").unwrap_or("0.0.0.0:3000".to_string());

    let database_url = env::var("DATABASE_URL").expect("failed to get database url from .env");

    let pool: Pool<Postgres> = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await
        .expect("failed to connect to the postgres pool");

    let session_store = PostgresStore::new(pool.clone());
    session_store
        .migrate()
        .await
        .expect("faild to migrate session store");

    let session_layer = SessionManagerLayer::new(session_store);
    let auth_backend = auth::axum_login::Backend::new(pool.clone());
    let auth_layer = AuthManagerLayerBuilder::new(auth_backend, session_layer).build();

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS user_events (
            id BIGSERIAL PRIMARY KEY,
            user_id BYTEA NOT NULL,
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
            journal_id BYTEA NOT NULL,
            event_type SMALLINT NOT NULL,
            payload BYTEA NOT NULL,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
            )",
    )
    .execute(&pool)
    .await
    .expect("failed to create the journal events table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS auth_users (
            user_id BYTEA NOT NULL,
            username TEXT NOT NULL,
            password TEXT NOT NULL
            )",
    )
    .execute(&pool)
    .await
    .expect("failed to create the auth user table");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS username_events (
                id BIGSERIAL PRIMARY KEY,
                user_id BYTEA NOT NULL,
                username VARCHAR(64) NOT NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now()
                )",
    )
    .execute(&pool)
    .await
    .expect("failed to create the username events table");

    let auth_routes = Router::new()
        .route("/login", get(auth::view::client_login))
        .route("/login", post(auth::login))
        .route("/logout", post(auth::log_out))
        .route("/signup", get(auth::view::client_signup))
        .route("/signup", post(auth::create_user));

    let webauthn_routes = webauthn::router();

    let journal_routes = Router::new()
        .route("/journal", get(journal::views::homepage::journal_list))
        .route("/createjournal", post(journal::commands::create_journal))
        .route(
            "/journal/{id}",
            get(journal::views::homepage::journal_detail),
        )
        .route(
            "/journal/{id}/transaction",
            get(journal::views::transaction::transaction_list_page),
        )
        .route(
            "/journal/{id}/account",
            get(journal::views::account::account_list_page),
        )
        .route(
            "/journal/{id}/person",
            get(journal::views::person::people_list_page),
        )
        .route_layer(login_required!(Backend, login_url = "/login"));

    // the dockerfile defines this for production deployments
    let site_root = std::env::var("SITE_ROOT").unwrap_or_else(|_| "target/site".to_string());

    let app = Router::new()
        .route("/favicon.ico", get(serve_favicon))
        .route("/logo.svg", get(serve_logo))
        .route_service(
            "/monkesto.css",
            ServeFile::new(format!("{}/pkg/monkesto.css", site_root)),
        )
        .route("/", get(Redirect::to("/journal")))
        .merge(auth_routes)
        .merge(journal_routes)
        .nest("/webauthn/", webauthn_routes)
        .fallback(notfoundpage::not_found_page)
        .layer(axum::Extension(pool))
        .layer(auth_layer);

    // run our app with hyper
    // `axum::Server` is a re-export of `hyper::Server`
    println!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind the tcp address");
    axum::serve(listener, app.into_make_service())
        .await
        .expect("failed to serve on the address");
}

async fn serve_favicon() -> impl IntoResponse {
    const FAVICON_BYTES: &[u8] = include_bytes!("favicon.ico");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/x-icon")],
        FAVICON_BYTES,
    )
}

async fn serve_logo() -> impl IntoResponse {
    const LOGO_SVG: &str = include_str!("logo.svg");
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "image/svg+xml")],
        LOGO_SVG,
    )
}
