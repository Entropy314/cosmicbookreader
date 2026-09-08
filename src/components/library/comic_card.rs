use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;

use crate::invoke;
use crate::state::{AppContext, ContextMenuState, ContextMenuTarget};
use crate::types::ComicBook;

#[component]
pub fn ComicCard(comic: ComicBook) -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();
    let navigate = use_navigate();

    // Store id as a Copy-friendly StoredValue so all derived closures are FnMut/Copy
    let id_sv = StoredValue::new(comic.id.clone());
    let title = comic.title.clone();

    // Load cover once on mount
    spawn_local(async move {
        let id = id_sv.get_value();
        if ctx.cover_cache.with_untracked(|c| c.contains_key(&id)) {
            return;
        }
        if let Ok(Some(data_uri)) = invoke::get_cover(&id).await {
            ctx.cover_cache.update(|c| { c.insert(id, data_uri); });
        }
    });

    let cover_uri    = move || ctx.cover_cache.with(|c| c.get(&id_sv.get_value()).cloned());
    let is_selected  = move || ctx.selected_comics.with(|s| s.contains(&id_sv.get_value()));
    let has_selection = move || ctx.selected_comics.with(|s| !s.is_empty());

    // Click: toggle selection when any are selected; navigate otherwise
    let on_click = move |_| {
        if has_selection() {
            let id = id_sv.get_value();
            ctx.selected_comics.update(|s| {
                if s.contains(&id) { s.remove(&id); } else { s.insert(id); }
            });
        } else {
            navigate(&format!("/read/{}", id_sv.get_value()), Default::default());
        }
    };

    // Right-click: bulk actions if this card is selected, single actions otherwise
    let on_contextmenu = move |e: leptos::ev::MouseEvent| {
        e.prevent_default();
        let target = if ctx.selected_comics.with(|s| s.contains(&id_sv.get_value())) {
            ContextMenuTarget::MultiSelect { ids: ctx.selected_comics.get().into_iter().collect() }
        } else {
            ContextMenuTarget::Comic { id: id_sv.get_value() }
        };
        ctx.context_menu.set(Some(ContextMenuState {
            x: e.client_x() as f64,
            y: e.client_y() as f64,
            target,
        }));
    };

    view! {
        <div
            class=move || if is_selected() { "comic-card selected" } else { "comic-card" }
            on:click=on_click
            on:contextmenu=on_contextmenu
        >
            <div class="comic-cover">
                {move || match cover_uri() {
                    Some(uri) => view! {
                        <img src=uri alt=title.clone() class="cover-img" loading="lazy" />
                    }.into_any(),
                    None => view! {
                        <div class="cover-placeholder">
                            <span class="placeholder-icon">"📖"</span>
                        </div>
                    }.into_any(),
                }}
                // Checkmark overlay — visible when any selection is active
                {move || has_selection().then(|| view! {
                    <div class=move || if is_selected() { "select-check checked" } else { "select-check" }>
                        {move || is_selected().then_some("✓")}
                    </div>
                })}
            </div>
            <div class="comic-info">
                <p class="comic-title">{comic.title.clone()}</p>
                <p class="comic-format">{format!("{:?}", comic.format).to_uppercase()}</p>
            </div>
        </div>
    }
}
