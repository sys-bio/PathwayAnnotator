//! PathwayAnnotator in the browser.
//!
//! Build and serve with Trunk from this directory:
//!
//! ```text
//! trunk serve --open
//! ```
//!
//! The knowledge pack (`packs/kpack.bin` at the repository root, built by
//! `kpack-build`) is copied next to the app and fetched on start.

mod browser;
mod session;
mod views;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(views::App);
}
