use leptos::prelude::*;

#[component]
pub fn EmptyState(on_open: Callback<()>) -> impl IntoView {
    view! {
        <div class="empty-state">
            <div class="empty-icon">"📚"</div>
            <h2>"No Comics Yet"</h2>
            <p>"Open a folder containing your comic books to get started."</p>
            <button class="btn-primary" on:click=move |_| on_open.run(())>
                "Open Folder"
            </button>
        </div>
    }
}
