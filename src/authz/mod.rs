mod grant;
mod memory;
mod projection;
mod role;
mod service;
mod sqlite;
mod store;

pub use grant::GrantId;
pub use grant::GrantPayload;
pub use role::RoleId;
pub use role::RolePayload;
pub use sqlite::AuthzSqliteService;
pub use sqlite::connect_service as connect_sqlite_service;

use axum::Router;
use axum_login::login_required;

pub fn router() -> Router<crate::StateType> {
    Router::new()
        .merge(role::router())
        .route_layer(login_required!(crate::BackendType, login_url = "/signin"))
}
