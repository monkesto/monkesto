mod disintegrate;
mod grant;
mod role;

pub use disintegrate::{AuthzEventStore, AuthzProjection, AuthzService};
pub use grant::GrantId;
pub use role::RoleId;

use axum::Router;
use axum_login::login_required;

pub fn router() -> Router<crate::StateType> {
    Router::new()
        .merge(role::router())
        .route_layer(login_required!(crate::BackendType, login_url = "/signin"))
}
