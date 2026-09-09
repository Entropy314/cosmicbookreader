#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(any(target_arch = "wasm32", test))]
mod components;
#[cfg(any(target_arch = "wasm32", test))]
mod hooks;
#[cfg(any(target_arch = "wasm32", test))]
mod invoke;
#[cfg(any(target_arch = "wasm32", test))]
mod state;
#[cfg(any(target_arch = "wasm32", test))]
mod types;
#[cfg(any(target_arch = "wasm32", test))]
mod reading;

#[cfg(target_arch = "wasm32")]
fn main() {
    console_error_panic_hook::set_once();
    leptos::prelude::mount_to_body(app::App)
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("This package is the browser frontend. Start the desktop app with: cargo tauri dev");
    std::process::exit(1);
}
