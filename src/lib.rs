#[cfg(feature = "ssr")]
pub mod api;

#[cfg(feature = "ssr")]
#[allow(dead_code, unused_must_use)]
pub mod app;

#[cfg(feature = "ssr")]
pub mod event_sourcing;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_islands();
}
