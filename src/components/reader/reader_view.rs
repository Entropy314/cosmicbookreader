use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::{use_navigate, use_params_map};

use crate::hooks::use_comic_reader::use_comic_reader;
use crate::hooks::use_keyboard::use_keyboard_shortcuts;
use crate::invoke;
use crate::state::{load_rtl, save_rtl, sibling_chapter, AppContext};

use super::nav_controls::NavControls;
use super::page_display::PageDisplay;
use super::reader_toolbar::ReaderToolbar;

#[component]
pub fn ReaderView() -> impl IntoView {
    let params = use_params_map();

    // Rebuild the pane whenever the route's comic changes. Leptos would
    // otherwise reuse the component, leaving it pinned to the chapter it was
    // first created with.
    view! {
        {move || {
            let comic_id = params.get().get("comic_id").unwrap_or_default();
            view! { <ReaderPane comic_id=comic_id /> }
        }}
    }
}

#[component]
fn ReaderPane(comic_id: String) -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();
    let reader = use_comic_reader(comic_id);
    let navigate = use_navigate();

    Effect::new(move |_| {
        if reader.page_data.with(|page| page.is_some()) {
            let id = reader.comic_id.get_value();
            if ctx.last_opened.get_untracked().as_deref() != Some(&id) {
                crate::state::save_setting("last_opened_book", &id);
                ctx.last_opened.set(Some(id));
            }
        }
    });

    // Reading direction is a property of the series, not the chapter.
    let series = StoredValue::new(ctx.library.with_untracked(|lib| {
        let id = reader.comic_id.get_value();
        lib.iter().find(|c| c.id == id).map(|c| c.series.clone())
    }));
    if let Some(name) = series.get_value() {
        reader.rtl.set(load_rtl(&name));
    }

    let toggle_rtl = Callback::new(move |()| {
        reader.rtl.update(|v| *v = !*v);
        if let Some(name) = series.get_value() {
            save_rtl(&name, reader.rtl.get_untracked());
        }
    });

    let toggle_fullscreen = Callback::new(move |()| {
        spawn_local(async move {
            let _ = invoke::toggle_fullscreen().await;
        });
    });

    // Jump to the chapter `offset` places away in this series.
    let go_chapter = Callback::new(move |offset: isize| {
        if let Some(id) = sibling_chapter(ctx, &reader.comic_id.get_value(), offset) {
            navigate(&format!("/read/{id}"), Default::default());
        }
    });

    // Turning past either end of a chapter rolls into the neighbouring one.
    // Chapters are often only a handful of pages, so this is the common case.
    let advance = Callback::new(move |forward: bool| {
        if reader.page_data.with_untracked(|page| page.is_none()) {
            return;
        }
        let page = reader.current_page.get_untracked();
        if forward {
            if page + 1 < reader.page_count.get_untracked() {
                reader.next_page();
            } else {
                go_chapter.run(1);
            }
        } else if page > 0 {
            reader.prev_page();
        } else {
            go_chapter.run(-1);
        }
    });

    use_keyboard_shortcuts(reader, advance, go_chapter, toggle_fullscreen);

    view! {
        <div class="reader-view">
            // Error display
            {move || reader.error.get().map(|e| view! {
                <div class="reader-error">
                    <p>{e}</p>
                    <a href="/">"← Back to Library"</a>
                </div>
            })}

            // Page + nav zones
            <div class="reader-scroll">
                <PageDisplay reader=reader />
                <NavControls reader=reader advance=advance />
            </div>

            // Toolbar
            <div
                class="reader-toolbar-wrapper"
                class:hidden=move || !reader.toolbar_visible.get()
            >
                <ReaderToolbar
                    reader=reader
                    advance=advance
                    go_chapter=go_chapter
                    toggle_rtl=toggle_rtl
                    toggle_fullscreen=toggle_fullscreen
                />
            </div>
        </div>
    }
}
