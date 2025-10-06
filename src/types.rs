#[cfg(feature = "ssr")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: i64,
    pub title: String,
    pub balance_cents: i64,
}
