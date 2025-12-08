use leptos::prelude::*;
use leptos_meta::{MetaTags, Stylesheet, Title, provide_meta_context};
use leptos_router::{
    StaticSegment,
    components::{Route, Router, Routes},
};
use crate::auth::ClientLogin;
use crate::auth::ClientSignUp;
use crate::home::HomePage;
use crate::transaction::Transact;
use crate::journal::GeneralJournal;
use crate::journal::JournalInvites;

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <AutoReload options=options.clone() />
                <HydrationScripts options />
                <MetaTags />
            </head>
            <body>
                <App />
            </body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    // Provides context that manages stylesheets, titles, meta tags, etc.
    provide_meta_context();

    view! {
        <!DOCTYPE html>
        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/prototype.css" />

        <Title text="Monkesto" />

        // content for this welcome page

        <Router>
            <main>
                <head>
                    <Routes fallback=|| "Page not found.".into_view()>
                        <Route path=StaticSegment("") view=HomePage />
                        <Route path=StaticSegment("/transact") view=Transact />
                        <Route path=StaticSegment("/journal") view=GeneralJournal />
                        <Route path=StaticSegment("/login") view=ClientLogin />
                        <Route path=StaticSegment("/signup") view=ClientSignUp />
                        <Route path=StaticSegment("/invites") view=JournalInvites />
                    </Routes>
                </head>
            </main>
        </Router>
    }
}
