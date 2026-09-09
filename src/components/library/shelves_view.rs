use std::collections::BTreeMap;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::invoke;
use crate::state::{matches_search, AppContext, Availability, ContextMenuState, ContextMenuTarget};
use crate::types::ComicBook;

use super::series_detail_view::SeriesDetailView;

fn group_series(
    comics: &[ComicBook],
    query: &str,
    availability: Availability,
) -> Vec<(String, Vec<ComicBook>)> {
    let query = query.trim().to_lowercase();
    let mut groups: BTreeMap<String, Vec<ComicBook>> = BTreeMap::new();
    for comic in comics {
        groups
            .entry(comic.series.clone())
            .or_default()
            .push(comic.clone());
    }
    groups
        .into_iter()
        .filter(|(_, books)| {
            books
                .iter()
                .any(|book| matches_search(book, &query, availability))
        })
        .collect()
}

/// Always-mounted Shelves component. Switches between the series-card grid
/// and the detail view internally so Leptos never has to reconcile two
/// different top-level branches when navigating between series.
#[component]
pub fn ShelvesView() -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();

    view! {
        {move || match ctx.selected_series.get() {
            // ── Series detail ──────────────────────────────────────────
            // Pass the series name as a prop so the component is
            // re-executed (not just reconciled in place) when the series
            // changes, giving every internal closure a fresh capture.
            Some(series) => view! {
                <SeriesDetailView series=series />
            }.into_any(),

            // ── Series grid ────────────────────────────────────────────
            None => {
                let series_list = Memo::new(move |_| {
                    ctx.library.with(|lib| group_series(lib, &ctx.search_query.get(), ctx.availability.get()))
                });

                view! {
                    <div class="title-library">
                    <div class="title-library-header">
                        <p class="eyebrow">"YOUR LIBRARY"</p><h1>"Find your next read"</h1>
                        <p>{move || series_list.with(|groups| {
                            let titles = groups.len();
                            let books: usize = groups.iter().map(|(_, books)| books.len()).sum();
                            format!("{titles} {} · {books} {}", if titles == 1 { "title" } else { "titles" }, if books == 1 { "book" } else { "books" })
                        })}</p>
                        <p>"Choose a title to explore its chapters, volumes, and issues."</p>
                    </div>
                    <Show when=move || series_list.with(Vec::is_empty)>
                        <super::empty_state::NoResults />
                    </Show>
                    <div class="comic-grid title-grid">
                        <For
                            each=move || series_list.get()
                            key=|(series, comics)| (series.clone(), comics.iter().map(|c| (c.id.clone(), c.path.clone(), c.title.clone(), c.downloaded)).collect::<Vec<_>>())
                            children=move |(series, comics)| view! {
                                <SeriesCard series=series comics=comics />
                            }
                        />
                    </div>
                    </div>
                }.into_any()
            }
        }}
    }
}

#[component]
fn SeriesCard(series: String, comics: Vec<ComicBook>) -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();
    let count = comics.len();
    let issue_label = if count == 1 {
        "1 book".to_string()
    } else {
        format!("{count} books")
    };
    let downloaded = comics.iter().filter(|c| c.downloaded).count();
    let availability = if downloaded == count {
        "Available offline".to_string()
    } else if downloaded == 0 && comics.iter().all(|c| c.id.starts_with("drive-")) {
        "On Google Drive".to_string()
    } else {
        format!("{downloaded} available offline")
    };
    let cover_book = comics
        .iter()
        .find(|c| c.downloaded)
        .or_else(|| comics.first());
    let first_id = cover_book.map(|c| c.id.clone()).unwrap_or_default();

    // Prefer an offline book; requesting a cover never downloads an archive.
    let load_id = first_id.clone();
    spawn_local(async move {
        let already = ctx.cover_cache.with_untracked(|c| c.contains_key(&load_id));
        if !already {
            if let Ok(Some(uri)) = invoke::get_cover(&load_id).await {
                ctx.cover_cache.update(|c| {
                    c.insert(load_id, uri);
                });
            }
        }
    });

    let cover_id = StoredValue::new(first_id.clone());
    let cover_uri = move || {
        ctx.cover_cache
            .with(|c| c.get(&cover_id.get_value()).cloned())
    };

    let series_sv = StoredValue::new(series.clone());
    let initials: String = series
        .split_whitespace()
        .take(2)
        .filter_map(|word| word.chars().next())
        .flat_map(char::to_uppercase)
        .collect();
    let hue = series
        .bytes()
        .fold(0u32, |sum, b| sum.wrapping_mul(31).wrapping_add(b as u32))
        % 360;
    let on_click = move |_| {
        ctx.series_query.set(String::new());
        ctx.selected_comics.update(|selected| selected.clear());
        ctx.selected_series.set(Some(series_sv.get_value()));
    };

    let ctx_ids: Vec<String> = comics.iter().map(|c| c.id.clone()).collect();
    let on_contextmenu = move |e: leptos::ev::MouseEvent| {
        e.prevent_default();
        ctx.context_menu.set(Some(ContextMenuState {
            x: e.client_x() as f64,
            y: e.client_y() as f64,
            target: ContextMenuTarget::Series {
                ids: ctx_ids.clone(),
            },
        }));
    };

    view! {
        <article class="series-card" on:contextmenu=on_contextmenu.clone()>
            <button type="button" class="series-open" on:click=on_click aria-label=format!("Browse {count} books in {series}")>
                <span class="series-cover" style=format!("--cover-hue: {hue}")>
                    {move || match cover_uri() {
                        Some(uri) => view! { <img class="cover-img" src=uri alt="" loading="lazy" /> }.into_any(),
                        None => view! { <span class="series-initials" aria-hidden="true">{initials.clone()}</span> }.into_any(),
                    }}
                </span>
                <span class="series-card-info">
                    <span class="comic-title">{series_sv.get_value()}</span>
                    <span class="comic-format">{issue_label}</span>
                    <span class="series-availability" class:offline=(downloaded > 0)>{availability}</span>
                </span>
                <span class="series-browse-arrow" aria-hidden="true">"→"</span>
            </button>
            <button class="icon-button series-more" on:click=on_contextmenu.clone()
                aria-label=format!("Actions for {series}")>"⋯"</button>
        </article>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ComicFormat;

    #[test]
    fn chapter_search_keeps_the_whole_title_group_and_reading_order() {
        let comic = |title: &str, series: &str| ComicBook {
            id: title.into(),
            path: String::new(),
            title: title.into(),
            series: series.into(),
            format: ComicFormat::Cbz,
            page_count: None,
            downloaded: false,
        };
        let books = vec![
            comic("Saga 1", "Saga"),
            comic("Saga 2", "Saga"),
            comic("Batman 1", "Batman"),
        ];
        let groups = group_series(&books, "  SAGA 2 ", Availability::All);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "Saga");
        assert_eq!(groups[0].1, books[..2]);
        assert_eq!(group_series(&books, "", Availability::All).len(), 2);
        assert!(group_series(&books, "unknown", Availability::All).is_empty());
        let mut mixed = books[..2].to_vec();
        mixed[0].downloaded = true;
        mixed[1].id = "drive-cloud".into();
        assert_eq!(group_series(&mixed, "Saga", Availability::Offline)[0].1.len(), 2);
        assert!(group_series(&mixed, "Saga 2", Availability::Offline).is_empty());
        assert_eq!(group_series(&mixed, "Saga 2", Availability::Cloud).len(), 1);
    }
}
