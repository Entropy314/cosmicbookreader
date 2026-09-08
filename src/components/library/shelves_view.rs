use std::collections::BTreeMap;

use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::invoke;
use crate::state::{AppContext, ContextMenuState, ContextMenuTarget};
use crate::types::ComicBook;

use super::series_detail_view::SeriesDetailView;

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
                let series_list = move || {
                    let query = ctx.search_query.get().to_lowercase();
                    ctx.library.with(|lib| {
                        let mut map: BTreeMap<String, Vec<ComicBook>> = BTreeMap::new();
                        for comic in lib {
                            if query.is_empty()
                                || comic.series.to_lowercase().contains(&query)
                                || comic.title.to_lowercase().contains(&query)
                            {
                                map.entry(comic.series.clone()).or_default().push(comic.clone());
                            }
                        }
                        map.into_iter().collect::<Vec<_>>()
                    })
                };

                view! {
                    <div class="comic-grid">
                        <For
                            each=series_list
                            key=|(series, _)| series.clone()
                            children=move |(series, comics)| view! {
                                <SeriesCard series=series comics=comics />
                            }
                        />
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
        "1 issue".to_string()
    } else {
        format!("{count} issues")
    };
    let first_id = comics.first().map(|c| c.id.clone()).unwrap_or_default();

    // Load cover for the first comic in this series
    let load_id = first_id.clone();
    spawn_local(async move {
        let already = ctx.cover_cache.with_untracked(|c| c.contains_key(&load_id));
        if !already {
            if let Ok(Some(uri)) = invoke::get_cover(&load_id).await {
                ctx.cover_cache.update(|c| { c.insert(load_id, uri); });
            }
        }
    });

    let cover_id = StoredValue::new(first_id.clone());
    let cover_uri = move || ctx.cover_cache.with(|c| c.get(&cover_id.get_value()).cloned());

    let series_sv = StoredValue::new(series.clone());
    let on_click = move |_| ctx.selected_series.set(Some(series_sv.get_value()));

    let ctx_ids: Vec<String> = comics.iter().map(|c| c.id.clone()).collect();
    let on_contextmenu = move |e: leptos::ev::MouseEvent| {
        e.prevent_default();
        ctx.context_menu.set(Some(ContextMenuState {
            x: e.client_x() as f64,
            y: e.client_y() as f64,
            target: ContextMenuTarget::Series { ids: ctx_ids.clone() },
        }));
    };

    view! {
        <div class="comic-card series-card" on:click=on_click on:contextmenu=on_contextmenu>
            <div class="comic-cover">
                {move || match cover_uri() {
                    Some(uri) => view! {
                        <img class="cover-img" src=uri alt="Series cover" />
                    }.into_any(),
                    None => view! {
                        <div class="cover-placeholder">
                            <span class="placeholder-icon">"📚"</span>
                        </div>
                    }.into_any(),
                }}
                <span class="series-count-badge">{count}</span>
            </div>
            <div class="comic-info">
                <div class="comic-title">{series}</div>
                <div class="comic-format">{issue_label}</div>
            </div>
        </div>
    }
}
