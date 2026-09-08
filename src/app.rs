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
