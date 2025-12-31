use leptos::{
    IntoView,
    prelude::{CustomAttribute, ElementChild, IntoAny, ServerFnError},
    view,
};

use crate::known_errors::KnownErrors;

pub fn handle_error(err: ServerFnError, context: &str) -> impl IntoView {
    use KnownErrors::*;
    if let Some(e) = KnownErrors::parse_error(&err) {
        match e {
            NotLoggedIn | SessionIdNotFound => {
                view! { <meta http-equiv="refresh" content="0; url=/login" /> }.into_any()
            }

            _ => view! {
                <p>
                    "An error occurred while " {context} " : "
                    {e.to_string().unwrap_or("failed to decode error".to_string())}
                </p>
            }
            .into_any(),
        }
    } else {
        view! { <p>"An unknown error occurred while " {context} " : " {err.to_string()}</p> }
            .into_any()
    }
}

#[macro_export]
macro_rules! unwrap_or_handle_error {
    ($res:expr, $context:expr) => {
        match $res {
            Ok(s) => s,
            Err(e) => return handle_error(e, $context).into_any(),
        }
    };
}
