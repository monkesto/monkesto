use leptos::prelude::*;
use leptos_meta::MetaTags;
use leptos_meta::Stylesheet;
use leptos_meta::Title;
use leptos_meta::provide_meta_context;
use leptos_router::components::Route;
use leptos_router::components::Router;
use leptos_router::components::Routes;
use leptos_router::path;

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
                    <Route
                        path=path!("/")
                        view=move || {
                            view! { <meta http-equiv="refresh" content="0; url=/journal" /> }
                                .into_view()
                        }
                    />
                </Routes>
            </main>
        </Router>
    }
}
