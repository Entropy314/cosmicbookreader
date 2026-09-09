use crate::state::{AppContext, Availability};
use leptos::prelude::*;

#[component]
pub fn EmptyState(on_open: Callback<()>) -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();
    view! {
        <div class="empty-state">
            <span class="empty-library-mark" aria-hidden="true">"＋"</span>
            <h1>"Your next read starts here"</h1>
            <p>"Bring your manga and comics together. Add a folder from your computer or connect a collection on Google Drive."</p>
            <div class="empty-actions">
                <button class="btn-primary" on:click=move |_| ctx.drive_panel_open.set(true)>"Connect Google Drive"</button>
                <button class="btn-secondary" on:click=move |_| on_open.run(())>"Open a local folder"</button>
            </div>
            <p class="empty-hint">"Drive books download when you read them. Your collection stays organized by title."</p>
        </div>
    }
}

#[component]
pub fn NoResults() -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();
    view! {
        <div class="no-results" role="status">
            <h2>"No books match these filters"</h2>
            <p>"Try another title or chapter, or show your whole collection."</p>
            <button class="btn-secondary" on:click=move |_| {
                if ctx.selected_series.get_untracked().is_some() { ctx.series_query.set(String::new()); }
                else { ctx.search_query.set(String::new()); }
                ctx.availability.set(Availability::All);
                ctx.selected_comics.update(|s| s.clear());
            }>"Clear search and filters"</button>
        </div>
    }
}
