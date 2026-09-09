use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::invoke;
use crate::reading::reading_target;
use crate::types::ReadingStatus;
use crate::state::{matches_search, save_directory, AppContext, Availability, ViewMode};

use super::comic_card::BookList;
use super::empty_state::EmptyState;
use super::shelves_view::ShelvesView;
use super::toolbar::LibraryToolbar;

#[component]
pub fn LibraryView() -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();
    let visible_books = Memo::new(move |_| {
        ctx.library.with(|books| {
            books
                .iter()
                .filter(|c| matches_search(c, &ctx.search_query.get(), ctx.availability.get()))
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    let resume = Memo::new(move |_| {
        let id = ctx.last_opened.get();
        ctx.library.with(|books| {
            let recent = books.iter().find(|c| Some(&c.id) == id.as_ref()).or_else(||
                books.iter().filter(|c| c.reading.updated_at > 0).max_by_key(|c| c.reading.updated_at))?;
            let series: Vec<_> = books.iter().filter(|c| c.series == recent.series).cloned().collect();
            reading_target(&series).cloned()
        })
    });
    let keyboard = window_event_listener(leptos::ev::keydown, move |e| {
        if event_target::<web_sys::Element>(&e)
            .closest("input, textarea, select, [contenteditable]")
            .ok()
            .flatten()
            .is_some()
        {
            return;
        }
        if (e.ctrl_key() || e.meta_key()) && e.key() == "a" {
            let series = ctx.selected_series.get_untracked();
            if series.is_none() && ctx.view_mode.get_untracked() == ViewMode::Shelves {
                return;
            }
            e.prevent_default();
            let query = if series.is_some() {
                ctx.series_query.get_untracked()
            } else {
                ctx.search_query.get_untracked()
            };
            let ids = ctx.library.with_untracked(|books| {
                books
                    .iter()
                    .filter(|c| {
                        series.as_ref().is_none_or(|s| &c.series == s)
                            && matches_search(c, &query, ctx.availability.get_untracked())
                    })
                    .map(|c| c.id.clone())
                    .collect()
            });
            ctx.selected_comics.set(ids);
        } else if e.key() == "Escape" {
            ctx.selected_comics.update(|s| s.clear());
        }
    });
    on_cleanup(move || keyboard.remove());

    // On mount: paint the persisted library, then refresh it from disk in the
    // background so a fast start can't show a stale list.
    if ctx.library.with_untracked(|l| l.is_empty()) {
        spawn_local(async move {
            ctx.is_loading_library.set(true);
            if let Ok(comics) = invoke::get_library().await {
                ctx.library.set(comics);
            }
            ctx.is_loading_library.set(false);

            // Rescan every folder the library draws from, so chapters added
            // to any of them appear without re-picking the folder.
            if let Ok(comics) = invoke::refresh_library().await {
                ctx.library.set(comics);
            }
        });
    }

    let open_folder = Callback::new(move |()| {
        spawn_local(async move {
            match invoke::pick_directory().await {
                Ok(Some(dir)) => {
                    ctx.is_loading_library.set(true);
                    ctx.current_directory.set(Some(dir.clone()));
                    ctx.selected_series.set(None);
                    save_directory(&dir);

                    match invoke::scan_directory(&dir).await {
                        Ok(comics) => {
                            ctx.library.set(comics);
                        }
                        Err(e) => ctx.error_message.set(Some(e)),
                    }
                    ctx.is_loading_library.set(false);
                }
                Ok(None) => {}
                Err(e) => ctx.error_message.set(Some(e)),
            }
        });
    });

    let add_files = Callback::new(move |()| {
        spawn_local(async move {
            match invoke::pick_files().await {
                Ok(comics) => ctx.library.set(comics),
                Err(e) => ctx.error_message.set(Some(e)),
            }
        });
    });

    let library_empty = move || ctx.library.with(|l| l.is_empty());
    let is_loading = move || ctx.is_loading_library.get();

    view! {
        <div class="library-view">
            <LibraryToolbar on_open=open_folder on_add_files=add_files />
            {move || ctx.drive_panel_open.get().then(|| view! { <super::drive_panel::DrivePanel /> })}

            // Error banner
            {move || ctx.error_message.get().map(|msg| view! {
                <div class="error-banner" role="alert">
                    <span>{msg}</span>
                    <button class="error-close" aria-label="Dismiss error" on:click=move |_| ctx.error_message.set(None)>"×"</button>
                </div>
            })}

            // Loading overlay
            {move || is_loading().then(|| view! {
                <div class="library-loading" role="status">
                    <div class="spinner" aria-hidden="true"></div>
                    <p>"Updating your library…"</p>
                </div>
            })}

            <Show when=move || ctx.selected_series.get().is_none() && ctx.search_query.get().is_empty()
                && ctx.availability.get() == Availability::All>
                {move || resume.get().map(|book| view! {
                    <a class="continue-reading" href=format!("/read/{}", book.id)>
                        <span class="continue-symbol" aria-hidden="true">"↗"</span>
                        <span><strong>{if book.reading.status == ReadingStatus::Reading { "Continue reading" } else { "Read next unread" }}</strong><span>{book.title}</span></span>
                        <span class="continue-action">{if book.reading.status == ReadingStatus::Reading { "Resume →" } else { "Read →" }}</span>
                    </a>
                })}
            </Show>

            // Library content
            {move || {
                if library_empty() && !is_loading() {
                    view! { <EmptyState on_open=open_folder /> }.into_any()
                } else if ctx.view_mode.get() == ViewMode::Shelves {
                    view! { <ShelvesView /> }.into_any()
                } else {
                    view! {
                        <section class="all-books-view">
                            <div class="title-library-header"><p class="eyebrow">"YOUR LIBRARY"</p><h1>"All books"</h1>
                                <p>{move || format!("Showing {} of {} books", visible_books.with(Vec::len), ctx.library.with(Vec::len))}</p>
                            </div>
                            <BookList comics=visible_books />
                        </section>
                    }.into_any()
                }
            }}
        </div>
    }
}
