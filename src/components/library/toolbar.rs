use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::{
    invoke,
    state::{AppContext, Availability, ViewMode},
};

#[component]
pub fn LibraryToolbar(on_open: Callback<()>, on_add_files: Callback<()>) -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();
    let pending_sync = RwSignal::new(false);
    let query = move || {
        if ctx.selected_series.get().is_some() {
            ctx.series_query
        } else {
            ctx.search_query
        }
    };
    let select_view = move |mode| {
        ctx.selected_series.set(None);
        ctx.selected_comics.update(|selected| selected.clear());
        ctx.view_mode.set(mode);
    };
    let sync = move |_| {
        pending_sync.set(true);
        spawn_local(async move {
            if let Err(error) = invoke::sync_drive().await {
                ctx.error_message.set(Some(error));
            }
            invoke::refresh_drive_status(ctx).await;
            let _ = pending_sync.try_set(false);
        });
    };

    view! {
        <header class="library-header">
            <div class="library-toolbar">
                <button class="library-brand" on:click=move |_| select_view(ViewMode::Shelves) aria-label="Cosmic Book Reader library">
                    <span class="brand-mark" aria-hidden="true">"C"</span>
                    <span>"Cosmic"<small>"BOOK READER"</small></span>
                </button>
                <div class="library-actions">
                    <button class="btn-secondary drive-toggle"
                        aria-expanded=move || ctx.drive_panel_open.get().to_string()
                        aria-controls="drive-panel"
                        on:click=move |_| ctx.drive_panel_open.update(|open| *open = !*open)>
                        <span class="status-dot" class:connected=move || ctx.drive_status.get().connected aria-hidden="true"></span>
                        {move || {
                            let status = ctx.drive_status.get();
                            if status.error.is_some() { "Drive needs attention" }
                            else if status.busy { "Drive syncing…" }
                            else if status.connected { "Google Drive" }
                            else { "Connect Drive" }
                        }}
                    </button>
                    <Show when=move || ctx.drive_status.get().connected>
                        <button class="btn-secondary quick-sync" on:click=sync
                            disabled=move || pending_sync.get() || ctx.drive_status.get().busy>
                            {move || if pending_sync.get() || ctx.drive_status.get().busy { "Syncing…" } else { "Sync now" }}
                        </button>
                    </Show>
                    <details class="add-books-menu">
                        <summary class="btn-primary">"＋ Add books"</summary>
                        <div class="add-books-options">
                            <button on:click=move |e: leptos::ev::MouseEvent| {
                                if let Some(element) = event_target::<web_sys::Element>(&e).closest("details").ok().flatten() { let _ = element.remove_attribute("open"); }
                                on_open.run(());
                            }>"Open a folder"<small>"Include all its subfolders"</small></button>
                            <button on:click=move |e: leptos::ev::MouseEvent| {
                                if let Some(element) = event_target::<web_sys::Element>(&e).closest("details").ok().flatten() { let _ = element.remove_attribute("open"); }
                                on_add_files.run(());
                            }>"Choose files"<small>"Add individual books"</small></button>
                        </div>
                    </details>
                </div>
            </div>
            <div class="library-browse-controls">
                <div class="search-field">
                    <span aria-hidden="true">"⌕"</span>
                    <input type="search" class="search-input"
                        placeholder=move || if ctx.selected_series.get().is_some() { "Find a chapter in this title…" } else { "Find a title or chapter…" }
                        aria-label=move || if ctx.selected_series.get().is_some() { "Search this title" } else { "Search your library" }
                        prop:value=move || query().get()
                        on:input=move |e| { query().set(event_target_value(&e)); ctx.selected_comics.update(|s| s.clear()); }
                    />
                    <Show when=move || !query().get().is_empty()>
                        <button class="icon-button" aria-label="Clear search" on:click=move |_| {
                            query().set(String::new()); ctx.selected_comics.update(|s| s.clear());
                        }>"×"</button>
                    </Show>
                </div>
                <div class="availability-options" role="group" aria-label="Book availability">
                    {[(Availability::All, "Everything"), (Availability::Offline, "Available offline"), (Availability::Cloud, "Not downloaded")].into_iter().map(|(filter, label)| view! {
                        <button class="filter-button" aria-pressed=move || (ctx.availability.get() == filter).to_string()
                            on:click=move |_| { ctx.availability.set(filter); ctx.selected_comics.update(|s| s.clear()); }>{label}</button>
                    }).collect_view()}
                </div>
                <div class="view-options" role="group" aria-label="Library organization">
                    <button class="btn-view-toggle" aria-pressed=move || (ctx.view_mode.get() == ViewMode::Shelves).to_string()
                        on:click=move |_| select_view(ViewMode::Shelves)>"By title"</button>
                    <button class="btn-view-toggle" aria-pressed=move || (ctx.view_mode.get() == ViewMode::Grid).to_string()
                        on:click=move |_| select_view(ViewMode::Grid)>"All books"</button>
                </div>
            </div>
        </header>
    }
}
