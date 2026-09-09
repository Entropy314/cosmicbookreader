use super::comic_card::BookList;
use crate::state::{matches_search, AppContext};
use leptos::prelude::*;

#[component]
pub fn SeriesDetailView(series: String) -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();
    let series = StoredValue::new(series);
    let all_books = Memo::new(move |_| {
        ctx.library.with(|lib| {
            lib.iter()
                .filter(|c| series.with_value(|s| c.series == *s))
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    let visible = Memo::new(move |_| {
        all_books.with(|books| {
            books
                .iter()
                .filter(|c| matches_search(c, &ctx.series_query.get(), ctx.availability.get()))
                .cloned()
                .collect::<Vec<_>>()
        })
    });
    let resume = Memo::new(move |_| {
        visible.with(|books| {
            let last = ctx.last_opened.get();
            books
                .iter()
                .find(|c| {
                    Some(&c.id) == last.as_ref() && (c.downloaded || c.id.starts_with("drive-"))
                })
                .cloned()
        })
    });
    let first = Memo::new(move |_| {
        visible.with(|books| {
            books
                .iter()
                .find(|c| c.downloaded || c.id.starts_with("drive-"))
                .cloned()
        })
    });
    let on_back = move |_| {
        ctx.selected_comics.update(|s| s.clear());
        ctx.series_query.set(String::new());
        ctx.selected_series.set(None);
    };

    view! {
        <section class="series-detail" aria-label="Books in this title">
            <div class="series-detail-header">
                <button class="text-button" on:click=on_back>"← All titles"</button>
                <div class="series-title-row">
                    <div>
                        <p class="eyebrow">"YOUR COLLECTION"</p>
                        <h1 class="series-detail-title">{series.get_value()}</h1>
                        <p class="collection-description">{move || {
                            let total = all_books.with(Vec::len);
                            let offline = all_books.with(|books| books.iter().filter(|c| c.downloaded).count());
                            let shown = visible.with(Vec::len);
                            if shown != total { format!("Showing {shown} of {total} books · {offline} available offline") }
                            else { format!("{total} books · {offline} available offline") }
                        }}</p>
                    </div>
                    <Show when=move || first.get().is_some()>
                        <a class="btn-primary" href=move || resume.get().or_else(|| first.get()).map(|book| format!("/read/{}", book.id))>
                            {move || if resume.get().is_some() { "Continue reading →" }
                                else if first.get().map(|c| c.id) == all_books.with(|books| books.first().map(|c| c.id.clone())) { "Read from the start →" }
                                else { "Read first result →" }}
                        </a>
                    </Show>
                </div>
                <p class="collection-hint">"Choose a book below. Books on Drive download when opened."</p>
            </div>
            <BookList comics=visible />
        </section>
    }
}
