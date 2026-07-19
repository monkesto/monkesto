use serde::{Deserialize, Serialize};

#[derive(PartialEq, Copy, Clone, Debug, Serialize, Deserialize, Default)]
pub enum Status {
    #[default]
    NotFound,
    Valid,
    Deleted,
}

impl Status {
    pub fn valid(&self) -> bool {
        *self == Status::Valid
    }

    /// returns if the status is `Valid` or `Deleted`
    /// useful for checking id collision
    pub fn found(&self) -> bool {
        (*self == Status::Valid) | (*self == Status::Deleted)
    }
}
