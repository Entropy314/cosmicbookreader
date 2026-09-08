use std::collections::BTreeSet;

use leptos::prelude::*;

use crate::state::AppContext;

use super::comic_card::ComicCard;

#[component]
pub fn SeriesDetailView(series: String) -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();

    // Store the series name as a Copy-friendly value
    let series_sv = StoredValue::new(series);

    let series_comics = move || {
        let query = ctx.search_query.get().to_lowercase();
        ctx.library.with(|lib| {
            lib.iter()
                .filter(|c| {
                    series_sv.with_value(|s| c.series == *s)
                        && (query.is_empty() || c.title.to_lowercase().contains(&query))
                })
                .cloned()
                .collect::<Vec<_>>()
        })
    };

    // All series for the navigation strip
    let all_series = move || {
        ctx.library.with(|lib| {
            lib.iter()
                .map(|c| c.series.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        })
    };

    // Ctrl+A selects all visible; Escape clears selection.
    // Store handle in StoredValue so it lives for the component's lifetime.
    let _kbd = StoredValue::new(window_event_listener(leptos::ev::keydown, move |e| {
        if e.ctrl_key() && e.key() == "a" {
            e.prevent_default();
            let query = ctx.search_query.get_untracked().to_lowercase();
            let all_ids: std::collections::HashSet<String> = ctx.library.with_untracked(|lib| {
                lib.iter()
                    .filter(|c| {
                        series_sv.with_value(|s| c.series == *s)
                            && (query.is_empty() || c.title.to_lowercase().contains(&query))
                    })
                    .map(|c| c.id.clone())
                    .collect()
            });
            ctx.selected_comics.set(all_ids);
        } else if e.key() == "Escape" {
            ctx.selected_comics.update(|s| s.clear());
        }
    }));

    let on_back = move |_| {
        ctx.selected_comics.update(|s| s.clear());
        ctx.selected_series.set(None);
    };

    let selection_count = move || ctx.selected_comics.with(|s| s.len());

    view! {
        <div class="series-detail">
            <div class="series-detail-header">
                <button class="btn-back" on:click=on_back>
                    "← Back"
                </button>
                <h1 class="series-detail-title">
                    {move || series_sv.get_value()}
                </h1>
                {move || {
                    let count = selection_count();
                    (count > 0).then(|| view! {
                        <span class="selection-indicator">
                            {count}" selected  •  Esc to clear"
                        </span>
                    })
                }}
            </div>

            // Horizontal strip — click any chip to jump directly to that series
            <div class="series-nav-strip">
                <For
                    each=all_series
                    key=|s| s.clone()
                    children=move |s| {
                        let s_click = s.clone();
                        let s_label = s.clone();
                        view! {
                            <button
                                class=move || {
                                    let active = ctx.selected_series
                                        .with(|sel| sel.as_deref() == Some(s.as_str()));
                                    if active { "series-nav-chip active" } else { "series-nav-chip" }
                                }
                                on:click=move |_| {
                                    ctx.selected_comics.update(|sel| sel.clear());
                                    ctx.selected_series.set(Some(s_click.clone()));
                                }
                            >
                                {s_label}
                            </button>
                        }
                    }
                />
            </div>

            <div class="comic-grid">
                <For
                    each=series_comics
                    key=|comic| comic.id.clone()
                    children=move |comic| view! { <ComicCard comic=comic /> }
                />
            </div>
        </div>
    }
}
