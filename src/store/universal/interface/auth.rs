use crate::auth::user::UserId;
use crate::auth::user::UserState;
use crate::authority::Authority;
use crate::email::Email;
use crate::store::universal::error::StoreResult;
use axum_login::AuthnBackend;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::LazyLock;
use webauthn_rs::prelude::Uuid;

pub static DEV_USERS: LazyLock<HashMap<Email, (UserId, Uuid)>> = LazyLock::new(|| {
    let mut map = HashMap::with_capacity(2);
    map.insert(
        Email::try_new("pacioli@monkesto.com").expect("valid email"),
        (
            UserId::from_str("zk8m3p5q7r2n4v6x").expect("valid userid"),
            Uuid::parse_str("a1b2c3d4-e5f6-4a5b-8c9d-0e1f2a3b4c5d").expect("valid uuid"),
        ),
    );
    map.insert(
        Email::try_new("wedgwood@monkesto.com").expect("valid email"),
        (
            UserId::from_str("yj7l2o4p6q8s0u1w").expect("valid userid"),
            Uuid::parse_str("b2c3d4e5-f6a7-5b6c-9d0e-1f2a3b4c5d6e").expect("valid uuid"),
        ),
    );
    map
});

pub trait AuthInterface: Send + Sync + Clone + AuthnBackend + 'static {
    async fn create_user_with_id(
        &self,
        user_id: UserId,
        email: Email,
        webauthn_uuid: Uuid,
        authority: &Authority,
    ) -> StoreResult<()>;

    async fn create_user(
        &self,
        email: Email,
        webauthn_uuid: Uuid,
        authority: &Authority,
    ) -> StoreResult<UserId> {
        let user_id = UserId::new();
        self.create_user_with_id(user_id, email, webauthn_uuid, authority)
            .await?;
        Ok(user_id)
    }

    async fn get_state(&self, user_id: UserId) -> StoreResult<UserState>;

    async fn get_id_from_email(&self, email: Email) -> StoreResult<Option<UserId>>;

    async fn get_dev_users(&self) -> StoreResult<Vec<UserState>>;
}

#[cfg(test)]
mod tests {
    use crate::store::universal::diesel_sqlite::DieselSqliteStore;
    use crate::store::universal::diesel_sqlite_interface::DieselSqliteAuthInterface;
    use crate::store::universal::interface::auth::{AuthInterface, DEV_USERS};
    use crate::store::universal::interface::{TEST_AUTHORITY, TEST_EMAIL};
    use uuid::Uuid;

    async fn interface() -> DieselSqliteAuthInterface {
        let store = DieselSqliteStore::new(":memory:", 1).await;

        let (_, auth_interface, _, _) = store.interfaces().await;

        auth_interface
    }

    #[tokio::test]
    async fn dev_users_exist() {
        let auth_interface = interface().await;
        for (email, (id, webauthn_id)) in DEV_USERS.clone() {
            assert_eq!(
                auth_interface
                    .get_id_from_email(email.clone())
                    .await
                    .unwrap(),
                Some(id)
            );
            let state = auth_interface.get_state(id).await.unwrap();
            assert_eq!(state.webauthn_uuid.0, webauthn_id);
            assert_eq!(state.email, email);
        }
    }

    #[tokio::test]
    async fn user_creation() {
        let auth_interface = interface().await;
        let webauthn_uuid = Uuid::new_v4();
        let user_id = auth_interface
            .create_user(TEST_EMAIL.clone(), webauthn_uuid, &TEST_AUTHORITY)
            .await
            .unwrap();

        assert_eq!(
            auth_interface
                .get_id_from_email(TEST_EMAIL.clone())
                .await
                .unwrap(),
            Some(user_id)
        );

        let state = auth_interface.get_state(user_id).await.unwrap();

        assert_eq!(state.id, user_id);
        assert_eq!(state.email, TEST_EMAIL.clone());
        assert_eq!(state.webauthn_uuid.0, webauthn_uuid);
    }
}
