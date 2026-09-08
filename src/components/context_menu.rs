use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::invoke;
use crate::state::{AppContext, ContextMenuTarget};

#[component]
pub fn ContextMenu() -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();

    // Close on any click/contextmenu on the backdrop
    let close = move |_| ctx.context_menu.set(None);

    view! {
        {move || ctx.context_menu.get().map(|menu| {
            let (ids, remove_label, delete_label) = match menu.target {
                ContextMenuTarget::Comic { id } => (
                    vec![id],
                    "Remove from Library".to_string(),
                    "Delete File".to_string(),
                ),
                ContextMenuTarget::Series { ids } => (
                    ids,
                    "Remove Series from Library".to_string(),
                    "Delete All Files".to_string(),
                ),
                ContextMenuTarget::MultiSelect { ids } => {
                    let n = ids.len();
                    (ids, format!("Remove {n} from Library"), format!("Delete {n} Files"))
                }
            };

            // Both menu items do the same thing; only the backend call differs.
            let act = move |ids: Vec<String>, delete_file: bool| {
                ctx.context_menu.set(None);
                ctx.selected_comics.update(|s| s.clear());
                spawn_local(async move {
                    for id in &ids {
                        let _ = if delete_file {
                            invoke::delete_comic_file(id).await
                        } else {
                            invoke::remove_comic(id).await
                        };
                    }
                    ctx.library.update(|l| l.retain(|c| !ids.contains(&c.id)));
                    ctx.cover_cache.update(|c| c.retain(|k, _| !ids.contains(k)));
                });
            };
            let remove_ids = ids.clone();

            view! {
                // Invisible backdrop to catch outside clicks
                <div class="ctx-backdrop" on:click=close on:contextmenu=close />
                <div
                    class="context-menu"
                    style=format!("left: {}px; top: {}px;", menu.x, menu.y)
                    on:click=|e| e.stop_propagation()
                    on:contextmenu=|e| e.prevent_default()
                >
                    <button class="ctx-item" on:click=move |_| act(remove_ids.clone(), false)>
                        {remove_label}
                    </button>
                    <button class="ctx-item ctx-item-danger" on:click=move |_| act(ids.clone(), true)>
                        {delete_label}
                    </button>
                </div>
            }
        })}
    }
}
