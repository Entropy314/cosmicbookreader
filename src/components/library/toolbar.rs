use leptos::prelude::*;

use crate::state::{AppContext, ViewMode};

#[component]
pub fn LibraryToolbar(on_open: Callback<()>, on_add_files: Callback<()>) -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();

    let current_dir = move || {
        ctx.current_directory.with(|d| {
            d.as_deref()
                .and_then(|p| std::path::Path::new(p).file_name())
                .and_then(|n| n.to_str())
                .map(|s| s.to_string())
        })
    };

    let toggle_view = move |_| {
        ctx.selected_series.set(None);
        ctx.view_mode.update(|m| {
            *m = if *m == ViewMode::Grid { ViewMode::Shelves } else { ViewMode::Grid };
        });
    };

    let view_label = move || {
        if ctx.view_mode.get() == ViewMode::Grid { "☰ Series" } else { "⊞ Grid" }
    };

    view! {
        <div class="library-toolbar">
            <div class="toolbar-left">
                <button class="btn-primary" on:click=move |_| on_open.run(())>
                    "📁 Open Folder"
                </button>
                <button class="btn-secondary" on:click=move |_| on_add_files.run(())>
                    "＋ Add Files"
                </button>
                {move || current_dir().map(|d| view! {
                    <span class="current-dir">{d}</span>
                })}
            </div>
            <div class="toolbar-right">
                <button class="btn-view-toggle" on:click=toggle_view>
                    {view_label}
                </button>
                <input
                    type="search"
                    class="search-input"
                    placeholder="Search comics..."
                    prop:value=ctx.search_query
                    on:input=move |e| ctx.search_query.set(event_target_value(&e))
                />
            </div>
        </div>
    }
}
