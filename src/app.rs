use leptos::prelude::*;
use leptos_router::components::{Router, Routes, Route};
use leptos_router::path;

use crate::components::context_menu::ContextMenu;
use crate::components::library::LibraryView;
use crate::components::reader::ReaderView;
use crate::state::AppContext;

#[component]
pub fn App() -> impl IntoView {
    let ctx = AppContext::new();
    provide_context(ctx);

    // Backend sync continues across reader/library navigation. Polling only
    // transfers a library snapshot when a sync actually changes its revision.
    let polling = RwSignal::new(false);
    let poll = move || {
        if polling.get_untracked() { return; }
        polling.set(true);
        leptos::task::spawn_local(async move {
            crate::invoke::refresh_drive_status(ctx).await;
            polling.set(false);
        });
    };
    poll();
    if let Ok(interval) = set_interval_with_handle(poll, std::time::Duration::from_secs(2)) {
        on_cleanup(move || interval.clear());
    }

    view! {
        <Router>
            <Routes fallback=|| view! { <p class="not-found">"Page not found"</p> }>
                <Route path=path!("") view=LibraryView />
                <Route path=path!("/read/:comic_id") view=ReaderView />
            </Routes>
        </Router>
        <ContextMenu />
    }
}
