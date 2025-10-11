#[cfg(feature = "ssr")]
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Account {
    pub id: u32,
    pub title: String,
    pub balance_cents: i64,
}

#[cfg(feature = "ssr")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    pub id: u32,
    pub session_id: String,
    pub timestamp: i64,
}

#[cfg(feature = "ssr")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PartialTransaction {
    pub id: u32,
    pub session_id: u32,
    pub balance_diff_cents: i64,
}

#[cfg(feature = "ssr")]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BalanceUpdate {
    pub id: u32,
    pub balance_diff_cents: i64,
}

#[cfg(feature = "ssr")]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum TransactionResult {
    UPDATED,
    BALANCEMISMATCH,
}
