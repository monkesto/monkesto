use crate::authority::{Actor, Authority};
use crate::email::Email;
use crate::name::Name;
use std::sync::LazyLock;

pub mod account;
pub mod auth;
pub mod journal;
pub mod transaction;

static TEST_EMAIL: LazyLock<Email> =
    LazyLock::new(|| Email::try_new("test@example.com").expect("test email"));

static TEST_ACCT_NAME: LazyLock<Name> =
    LazyLock::new(|| Name::try_new("test account".to_string()).expect("test account name"));

static TEST_JOURNAL_NAME: LazyLock<Name> =
    LazyLock::new(|| Name::try_new("test journal".to_string()).expect("test journal name"));

const TEST_AUTHORITY: Authority = Authority::Direct(Actor::System);
