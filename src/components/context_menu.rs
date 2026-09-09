use crate::invoke;
use crate::state::{AppContext, ContextMenuTarget};
use crate::types::ReadingStatus;
use leptos::prelude::*;
use leptos::task::spawn_local;

#[component]
pub fn ContextMenu() -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();
    let close = move |_| ctx.context_menu.set(None);
    let keyboard = window_event_listener(leptos::ev::keydown, move |e| {
        if e.key() == "Escape" {
            ctx.context_menu.set(None);
        }
    });
    on_cleanup(move || keyboard.remove());

    view! {
        {move || ctx.context_menu.get().map(|menu| {
            let ids = match menu.target {
                ContextMenuTarget::Comic { id } => vec![id],
                ContextMenuTarget::Series { ids } | ContextMenuTarget::MultiSelect { ids } => ids,
            };
            let delete_ids: Vec<String> = ctx.library.with_untracked(|books| books.iter()
                .filter(|book| ids.contains(&book.id) && book.downloaded).map(|book| book.id.clone()).collect());
            let count = ids.len();
            let delete_count = delete_ids.len();
            let status_ids = StoredValue::new(ids.clone());
            let mark = move |status| {
                let ids = status_ids.get_value();
                ctx.context_menu.set(None);
                ctx.selected_comics.update(|s| s.clear());
                spawn_local(async move {
                    match invoke::set_reading_status(&ids, status).await {
                        Ok(()) => match invoke::get_library().await {
                            Ok(books) => ctx.library.set(books),
                            Err(error) => ctx.error_message.set(Some(error)),
                        },
                        Err(error) => ctx.error_message.set(Some(error)),
                    }
                });
            };
            let act = move |ids: Vec<String>, delete_file: bool| {
                if delete_file && !window().confirm_with_message(&format!("Delete {} local file(s) from this computer? This cannot be undone. Google Drive originals are kept.", ids.len())).unwrap_or(false) { return; }
                ctx.context_menu.set(None);
                ctx.selected_comics.update(|s| s.clear());
                spawn_local(async move {
                    let mut removed = Vec::new();
                    let mut errors = Vec::new();
                    for id in ids {
                        let result = if delete_file { invoke::delete_comic_file(&id).await } else { invoke::remove_comic(&id).await };
                        match result { Ok(()) => removed.push(id), Err(error) => errors.push(error) }
                    }
                    ctx.library.update(|books| books.retain(|c| !removed.contains(&c.id)));
                    ctx.cover_cache.update(|covers| covers.retain(|id, _| !removed.contains(id)));
                    if !errors.is_empty() { ctx.error_message.set(Some(format!("Some books could not be updated: {}", errors.join("; ")))); }
                    if let Some(series) = ctx.selected_series.get_untracked() {
                        if !ctx.library.with_untracked(|books| books.iter().any(|c| c.series == series)) { ctx.selected_series.set(None); }
                    }
                });
            };
            view! {
                <div class="ctx-backdrop" on:click=close on:contextmenu=close />
                <div class="context-menu" role="group" aria-label="Book actions"
                    style=format!("left: clamp(8px, {}px, calc(100vw - 250px)); top: clamp(8px, {}px, calc(100dvh - 290px));", menu.x, menu.y)
                    on:click=|e| e.stop_propagation() on:contextmenu=|e| e.prevent_default()>
                    <button class="ctx-item" on:click=move |_| mark(ReadingStatus::Completed)>"Mark completed"</button>
                    <button class="ctx-item" title="Reset reading status and saved page" on:click=move |_| mark(ReadingStatus::Unread)>"Mark unread · reset progress"</button>
                    <button class="ctx-item" on:click=move |_| act(ids.clone(), false)>
                        {if count == 1 { "Remove from library".into() } else { format!("Remove {count} books from library") }}
                    </button>
                    <button class="ctx-item ctx-item-danger" disabled=move || delete_count == 0 on:click=move |_| act(delete_ids.clone(), true)>
                        {if delete_count <= 1 { "Delete local file".into() } else { format!("Delete {delete_count} local files") }}
                    </button>
                    <p class="context-note">"Drive originals stay on Google Drive."</p>
                    <button class="ctx-item" on:click=close>"Close"</button>
                </div>
            }
        })}
    }
}
