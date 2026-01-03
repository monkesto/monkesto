use maud::{DOCTYPE, Markup, html};

pub fn header(content: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html class="h-full bg-white dark:bg-gray-900 text-gray-900 dark:text-white" {
            head {
                meta charset="UTF-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                script src="https://cdn.jsdelivr.net/npm/@tailwindcss/browser@4" {}
            }
            body {
                (content)
            }
        }
    }
}
