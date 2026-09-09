use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

use crate::hooks::use_comic_reader::ReaderState;

pub fn use_keyboard_shortcuts(
    reader: ReaderState,
    advance: Callback<bool>,
    go_chapter: Callback<isize>,
    toggle_fullscreen: Callback<()>,
) {
    let navigate = use_navigate();

    let listener = window_event_listener(leptos::ev::keydown, move |e| {
        if event_target::<web_sys::Element>(&e)
            .closest("input, textarea, select, [contenteditable]")
            .ok()
            .flatten()
            .is_some()
        {
            return;
        }
        // Reading right-to-left swaps which horizontal key moves forward; the
        // vertical and paging keys keep their usual meaning.
        let forward_is_right = !reader.rtl.get_untracked();

        match e.key().as_str() {
            "ArrowRight" => {
                e.prevent_default();
                advance.run(forward_is_right);
            }
            "ArrowLeft" => {
                e.prevent_default();
                advance.run(!forward_is_right);
            }
            "ArrowDown" | " " | "PageDown" => {
                e.prevent_default();
                advance.run(true);
            }
            "ArrowUp" | "PageUp" => {
                e.prevent_default();
                advance.run(false);
            }
            "Home" => {
                e.prevent_default();
                reader.go_to_page(0);
            }
            "End" => {
                e.prevent_default();
                let total = reader.page_count.get_untracked();
                if total > 0 {
                    reader.go_to_page(total - 1);
                }
            }
            "]" => go_chapter.run(1),
            "[" => go_chapter.run(-1),
            "+" | "=" => reader.zoom_in(),
            "-" => reader.zoom_out(),
            "f" | "F" => reader.cycle_fit_mode(),
            "F11" => {
                e.prevent_default();
                toggle_fullscreen.run(());
            }
            "t" | "T" => reader.toggle_toolbar(),
            "Escape" => {
                navigate("/", Default::default());
            }
            _ => {}
        }
    });
    on_cleanup(move || listener.remove());
}
