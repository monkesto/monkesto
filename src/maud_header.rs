use maud::{DOCTYPE, Markup, html};

pub async fn header(content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html class="h-full bg-white dark:bg-gray-900 text-gray-900 dark:text-white"{
            link rel="stylesheet" href="/pkg/monkesto.css";
            (content)
        }
    }
}
