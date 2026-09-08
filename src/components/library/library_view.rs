use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::invoke;
use crate::state::{AppContext, ViewMode, save_directory};

use super::comic_card::ComicCard;
use super::empty_state::EmptyState;
use super::shelves_view::ShelvesView;
use super::toolbar::LibraryToolbar;

#[component]
pub fn LibraryView() -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();

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

            // Error banner
            {move || ctx.error_message.get().map(|msg| view! {
                <div class="error-banner" on:click=move |_| ctx.error_message.set(None)>
                    <span>{msg}</span>
                    <button class="error-close">"×"</button>
                </div>
            })}

            // Loading overlay
            {move || is_loading().then(|| view! {
                <div class="loading-overlay">
                    <div class="spinner"></div>
                    <p>"Scanning comics..."</p>
                </div>
            })}

            // Library content
            {move || {
                if library_empty() && !is_loading() {
                    view! { <EmptyState on_open=open_folder /> }.into_any()
                } else if ctx.view_mode.get() == ViewMode::Shelves {
                    view! { <ShelvesView /> }.into_any()
                } else {
                    view! {
                        <div class="comic-grid">
                            <For
                                each=move || {
                                    let query = ctx.search_query.get().to_lowercase();
                                    ctx.library.with(|l| {
                                        if query.is_empty() {
                                            l.clone()
                                        } else {
                                            l.iter()
                                                .filter(|c| c.title.to_lowercase().contains(&query) || c.series.to_lowercase().contains(&query))
                                                .cloned()
                                                .collect()
                                        }
                                    })
                                }
                                key=|comic| comic.id.clone()
                                children=move |comic| view! { <ComicCard comic=comic /> }
                            />
                        </div>
                    }.into_any()
                }
            }}
        </div>
    }
}

