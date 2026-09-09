use super::comic_card::BookList;
use crate::reading::{SeriesProgress, next_unread, reading_target};
use crate::types::ReadingStatus;
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
    let progress = Memo::new(move |_| all_books.with(|books| SeriesProgress::from_books(books)));
    let target = Memo::new(move |_| visible.with(|books| reading_target(books).cloned()));
    let unread = Memo::new(move |_| visible.with(|books| next_unread(books).cloned()));
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
                    <div class="series-reading-actions">
                        <Show when=move || target.get().is_some()>
                            <a class="btn-primary" href=move || target.get().map(|book| format!("/read/{}", book.id))>
                                {move || if target.get().is_some_and(|b| b.reading.status == ReadingStatus::Reading) { "Continue reading →" } else { "Read next unread →" }}
                            </a>
                        </Show>
                        <Show when=move || target.get().is_some_and(|b| b.reading.status == ReadingStatus::Reading) && unread.get().is_some()>
                            <a class="btn-secondary" href=move || unread.get().map(|book| format!("/read/{}", book.id))>"Read next unread →"</a>
                        </Show>
                        <Show when=move || progress.with(|p| p.total > 0 && p.completed == p.total)>
                            <span class="all-read">"✓ All books completed"</span>
                        </Show>
                    </div>
                </div>
                <div class="series-progress-summary">
                    <span>{move || progress.get().label()}</span>
                    <progress aria-label="Completed books in this title" max=move || progress.get().total value=move || progress.get().completed></progress>
                </div>
                <p class="collection-hint">"Choose a book below. Books on Drive download when opened."</p>
            </div>
            <BookList comics=visible />
        </section>
    }
}
