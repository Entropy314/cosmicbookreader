use crate::hooks::use_comic_reader::{FitMode, ReaderState};
use crate::state::{save_setting, sibling_chapter, AppContext};
use leptos::prelude::*;
use leptos_router::hooks::use_navigate;

#[component]
pub fn ReaderToolbar(
    reader: ReaderState,
    advance: Callback<bool>,
    go_chapter: Callback<isize>,
    toggle_rtl: Callback<()>,
    toggle_fullscreen: Callback<()>,
) -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();
    let navigate = use_navigate();
    let forward_is_right = move || !reader.rtl.get();
    let title = move || {
        ctx.library.with(|books| {
            books
                .iter()
                .find(|b| b.id == reader.comic_id.get_value())
                .map(|b| b.title.clone())
                .unwrap_or_else(|| "Reading".into())
        })
    };
    let ready = move || reader.page_data.with(|p| p.is_some());
    let can_previous = move || {
        ready()
            && (reader.current_page.get() > 0
                || sibling_chapter(ctx, &reader.comic_id.get_value(), -1).is_some())
    };
    let can_next = move || {
        ready()
            && (reader.current_page.get() + 1 < reader.page_count.get()
                || sibling_chapter(ctx, &reader.comic_id.get_value(), 1).is_some())
    };

    view! {
        <div class="reader-controls">
            {move || reader.progress_error.get().map(|error| view! {
                <div class="progress-save-error" role="alert"><span>{format!("Reading progress could not be saved: {error}")}</span>
                    <button class="text-button" on:click=move |_| reader.retry_progress()>"Retry saving"</button>
                </div>
            })}
            <div class="reader-caption"><span title=title>{title}</span><span>"Progress saves automatically"</span></div>
            <div class="reader-toolbar">
                <div class="toolbar-left">
                    <button class="toolbar-btn" on:click=move |_| navigate("/", Default::default()) title="Back to library (Esc)">"← Library"</button>
                </div>
                <div class="toolbar-center">
                    <button class="toolbar-btn" on:click=move |_| go_chapter.run(-1) title="Previous chapter ([)" aria-label="Previous chapter"
                        disabled=move || !ready() || sibling_chapter(ctx, &reader.comic_id.get_value(), -1).is_none()>"⏮"</button>
                    <button class="toolbar-btn" on:click=move |_| advance.run(!forward_is_right())
                        title=move || if forward_is_right() { "Previous page" } else { "Next page" }
                        aria-label=move || if forward_is_right() { "Previous page" } else { "Next page" }
                        disabled=move || if forward_is_right() { !can_previous() } else { !can_next() }>"‹"</button>
                    <label class="page-indicator">"Page "<input type="number" min="1" max=move || reader.page_count.get()
                        aria-label="Go to page" disabled=move || !ready()
                        prop:value=move || if ready() { (reader.current_page.get() + 1).to_string() } else { "0".into() }
                        on:change=move |e| {
                            if let Ok(page) = event_target_value(&e).parse::<u32>() {
                                let count = reader.page_count.get_untracked();
                                if count > 0 { reader.go_to_page(page.clamp(1, count) - 1); }
                            }
                        } />" of "{move || reader.page_count.get()}</label>
                    <button class="toolbar-btn" on:click=move |_| advance.run(forward_is_right())
                        title=move || if forward_is_right() { "Next page" } else { "Previous page" }
                        aria-label=move || if forward_is_right() { "Next page" } else { "Previous page" }
                        disabled=move || if forward_is_right() { !can_next() } else { !can_previous() }>"›"</button>
                    <button class="toolbar-btn" on:click=move |_| go_chapter.run(1) title="Next chapter (])" aria-label="Next chapter"
                        disabled=move || !ready() || sibling_chapter(ctx, &reader.comic_id.get_value(), 1).is_none()>"⏭"</button>
                </div>
                <div class="toolbar-right">
                    <button class="toolbar-btn" on:click=move |_| reader.zoom_out() title="Zoom out (-)" aria-label="Zoom out">"−"</button>
                    <select class="reader-fit-select" aria-label="Page fit"
                        prop:value=move || match reader.fit_mode.get() { FitMode::FitPage => "page", FitMode::FitWidth => "width", FitMode::FitHeight => "height", FitMode::Free => "free" }
                        on:change=move |e| {
                            let value = event_target_value(&e);
                            reader.fit_mode.set(match value.as_str() { "width" => FitMode::FitWidth, "height" => FitMode::FitHeight, "free" => FitMode::Free, _ => FitMode::FitPage });
                            save_setting("fit_mode", &value);
                        }>
                        <option value="page">{FitMode::FitPage.label()}</option><option value="width">{FitMode::FitWidth.label()}</option><option value="height">{FitMode::FitHeight.label()}</option>
                        <option value="free">{move || format!("Zoom {:.0}%", reader.zoom.get() * 100.0)}</option>
                    </select>
                    <button class="toolbar-btn" on:click=move |_| reader.zoom_in() title="Zoom in (+)" aria-label="Zoom in">"＋"</button>
                    <button class="toolbar-btn" on:click=move |_| toggle_rtl.run(())
                        aria-label="Read right to left" aria-pressed=move || reader.rtl.get().to_string()
                        title="Change reading direction for this title">
                        {move || if reader.rtl.get() { "Right to left ←" } else { "Left to right →" }}
                    </button>
                    <button class="toolbar-btn" on:click=move |_| toggle_fullscreen.run(()) title="Fullscreen (F11)" aria-label="Toggle fullscreen">"⛶"</button>
                </div>
            </div>
        </div>
    }
}
