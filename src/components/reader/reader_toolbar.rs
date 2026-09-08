use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

use crate::hooks::use_comic_reader::{FitMode, ReaderState};

#[component]
pub fn ReaderToolbar(
    reader: ReaderState,
    advance: Callback<bool>,
    go_chapter: Callback<isize>,
    toggle_rtl: Callback<()>,
    toggle_fullscreen: Callback<()>,
) -> impl IntoView {
    let navigate = use_navigate();

    // Reading right-to-left, the on-screen arrows swap meaning: the left one
    // advances. The glyphs stay put so they still match the click zones.
    let forward_is_right = move || !reader.rtl.get();

    // Progress saves on every page turn and the archive closes on unmount, so
    // going back is just navigation.
    let on_back = move |_| navigate("/", Default::default());

    view! {
        <div class="reader-toolbar">
            <div class="toolbar-left">
                <button class="toolbar-btn" on:click=on_back title="Back to Library (Esc)">
                    "← Library"
                </button>
            </div>
            <div class="toolbar-center">
                <button
                    class="toolbar-btn"
                    on:click=move |_| go_chapter.run(-1)
                    title="Previous chapter ([)"
                >
                    "⏮"
                </button>
                <button
                    class="toolbar-btn"
                    on:click=move |_| advance.run(!forward_is_right())
                    title="Previous page"
                >
                    "‹"
                </button>
                <span class="page-indicator">
                    {move || format!("{} / {}", reader.current_page.get() + 1, reader.page_count.get())}
                </span>
                <button
                    class="toolbar-btn"
                    on:click=move |_| advance.run(forward_is_right())
                    title="Next page"
                >
                    "›"
                </button>
                <button
                    class="toolbar-btn"
                    on:click=move |_| go_chapter.run(1)
                    title="Next chapter (])"
                >
                    "⏭"
                </button>
            </div>
            <div class="toolbar-right">
                <button
                    class="toolbar-btn"
                    on:click=move |_| reader.zoom_out()
                    title="Zoom out (-)"
                >
                    "−"
                </button>
                <span class="zoom-label">
                    {move || {
                        if reader.fit_mode.get() == FitMode::Free {
                            format!("{:.0}%", reader.zoom.get() * 100.0)
                        } else {
                            reader.fit_mode.get().label().to_string()
                        }
                    }}
                </span>
                <button
                    class="toolbar-btn"
                    on:click=move |_| reader.zoom_in()
                    title="Zoom in (+)"
                >
                    "+"
                </button>
                <button
                    class="toolbar-btn fit-btn"
                    on:click=move |_| reader.cycle_fit_mode()
                    title="Cycle fit mode (F)"
                >
                    "⊡"
                </button>
                <button
                    class="toolbar-btn"
                    on:click=move |_| toggle_rtl.run(())
                    title=move || {
                        if reader.rtl.get() {
                            "Right to left (manga) - click for left to right"
                        } else {
                            "Left to right - click for right to left (manga)"
                        }
                    }
                >
                    {move || if reader.rtl.get() { "☚" } else { "☛" }}
                </button>
                <button
                    class="toolbar-btn"
                    on:click=move |_| toggle_fullscreen.run(())
                    title="Fullscreen (F11)"
                >
                    "⛶"
                </button>
            </div>
        </div>
    }
}
