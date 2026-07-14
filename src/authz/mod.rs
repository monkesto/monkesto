mod event;
mod grant;
mod role;
mod service;
mod store;

pub use grant::GrantId;
pub use role::{RoleId, RoleIndex};
pub use service::AuthzService;
pub use store::AuthzEventStore;

use axum::Router;
use axum_login::login_required;

pub fn router() -> Router<crate::StateType> {
    Router::new()
        .merge(role::router())
        .route_layer(login_required!(crate::BackendType, login_url = "/signin"))
}
