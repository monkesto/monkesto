use maud::DOCTYPE;
use maud::Markup;
use maud::html;

pub fn theme(content: Markup) -> Markup {
    theme_with_head(None, html! {}, content)
}

pub fn theme_with_head(title: Option<&str>, extra_head: Markup, content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" class="h-full bg-white dark:bg-gray-900 text-gray-900 dark:text-white" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                link rel="stylesheet" href="/monkesto.css";
                @if let Some(title) = title {
                    title { (title) " - Monkesto" }
                }
                (extra_head)
            }
            body {
                (content)
            }
        }
    }
}
