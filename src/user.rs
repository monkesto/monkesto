pub type UserId = uuid::Uuid;

pub struct User {
    id: UserId,
    username: String,
    name: String,
}

tokio::task_local! {
    pub static USER_ID: UserId;
    pub static USER: User;
}

pub enum UserEvent {
    Created { id: UserId, hashed_password: String },
    PasswordUpdated { id: UserId, hashed_password: String },
    Deleted { id: UserId },
}