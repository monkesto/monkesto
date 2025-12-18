use super::auth::ClientLogin;
use super::auth::ClientSignUp;
use super::home::HomePage;
use super::journal::GeneralJournal;
use super::journal::JournalInvites;
use super::journal::JournalList;
use super::transaction::Transact;
use leptos::prelude::*;
use leptos_meta::MetaTags;
use leptos_meta::Stylesheet;
use leptos_meta::Title;
use leptos_meta::provide_meta_context;
use leptos_router::StaticSegment;
use leptos_router::components::Route;
use leptos_router::components::Router;
use leptos_router::components::Routes;

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en" class="h-full bg-white dark:bg-gray-900 text-gray-900 dark:text-white">
            <head>
                <meta charset="utf-8" />
                <meta name="viewport" content="width=device-width, initial-scale=1" />
                <AutoReload options=options.clone() />
                <HydrationScripts options islands=true />
                <MetaTags />
            </head>
            <body class="h-full">
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
        // injects a stylesheet into the document <head>
        // id=leptos means cargo-leptos will hot-reload this stylesheet
        <Stylesheet id="leptos" href="/pkg/monkesto.css" />

        <Title text="Monkesto" />
        <Router>
            <main>
                <Routes fallback=|| "Page not found.".into_view()>
                    <Route path=StaticSegment("") view=HomePage />
                    <Route path=StaticSegment("/transact") view=Transact />
                    <Route path=StaticSegment("/journal") view=GeneralJournal />
                    <Route path=StaticSegment("/login") view=ClientLogin />
                    <Route path=StaticSegment("/signup") view=ClientSignUp />
                    <Route path=StaticSegment("/invites") view=JournalInvites />
                    <Route path=StaticSegment("/new_journal") view=JournalList />
                </Routes>
            </main>
        </Router>
    }
}
