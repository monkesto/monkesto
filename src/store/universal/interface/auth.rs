use crate::auth::user::UserId;
use crate::auth::user::UserState;
use crate::authority::Authority;
use crate::email::Email;
use crate::store::universal::error::StoreResult;
use axum_login::AuthnBackend;
use std::collections::HashSet;
use std::sync::LazyLock;
use webauthn_rs::prelude::Uuid;

pub static DEV_USERS: LazyLock<HashSet<Email>> = LazyLock::new(|| {
    let mut set = HashSet::with_capacity(2);
    set.insert(Email::try_new("pacioli@monkesto.com").expect("valid email"));
    set.insert(Email::try_new("wedgwood@monkesto.com").expect("valid email"));
    set
});

pub trait AuthInterface: Send + Sync + Clone + AuthnBackend + 'static {
    async fn create_user(
        &self,
        email: Email,
        webauthn_uuid: Uuid,
        authority: &Authority,
    ) -> StoreResult<UserId>;

    async fn get_user(&self, user_id: UserId) -> StoreResult<UserState>;

    async fn email_exists(&self, email: Email) -> StoreResult<bool>;

    async fn get_dev_users(&self) -> StoreResult<Vec<UserState>>;
}
