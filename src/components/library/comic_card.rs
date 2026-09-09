use leptos::prelude::*;

use crate::state::{AppContext, ContextMenuState, ContextMenuTarget};
use crate::types::{ComicBook, ReadingStatus};

/// A readable chapter row, including for books whose covers are not downloaded.
#[component]
pub fn ComicCard(comic: ComicBook) -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();
    let id = StoredValue::new(comic.id.clone());
    let title = StoredValue::new(comic.title.clone());
    let is_selected = move || ctx.selected_comics.with(|s| s.contains(&id.get_value()));
    let cloud = comic.id.starts_with("drive-") && !comic.downloaded;
    let available = comic.downloaded || cloud;
    let availability = if comic.downloaded {
        "Available offline"
    } else if cloud {
        "On Google Drive"
    } else {
        "File unavailable"
    };
    let action = if cloud {
        "Download & read"
    } else if comic.downloaded {
        match comic.reading.status {
            ReadingStatus::Unread => "Read",
            ReadingStatus::Reading => "Continue",
            ReadingStatus::Completed => "Read again",
        }
    } else {
        "Unavailable"
    };
    let href = format!("/read/{}", comic.id);
    let open_menu = move |e: leptos::ev::MouseEvent| {
        e.prevent_default();
        let target = if is_selected() {
            ContextMenuTarget::MultiSelect {
                ids: ctx.selected_comics.get_untracked().into_iter().collect(),
            }
        } else {
            ContextMenuTarget::Comic { id: id.get_value() }
        };
        ctx.context_menu.set(Some(ContextMenuState {
            x: e.client_x() as f64,
            y: e.client_y() as f64,
            target,
        }));
    };

    view! {
        <article class="book-row" class:selected=is_selected on:contextmenu=open_menu>
            <input class="book-select" type="checkbox" aria-label=format!("Select {}", comic.title)
                prop:checked=is_selected on:change=move |e| {
                    let checked = event_target_checked(&e);
                    ctx.selected_comics.update(|selected| {
                        if checked { selected.insert(id.get_value()); } else { selected.remove(&id.get_value()); }
                    });
                } />
            <span class="book-file-icon" aria-hidden="true">{format!("{:?}", comic.format).to_uppercase()}</span>
            <div class="book-row-info">
                <a class="book-name" href=href.clone() title=title.get_value()
                    on:click=move |e| if !available { e.prevent_default(); }>{comic.title}</a>
                <div class="book-row-meta">
                    <span>{comic.series}</span>
                    <span class="availability-label" class:offline=comic.downloaded>{availability}</span>
                </div>
                <div class="book-reading-progress">
                    <span class="reading-status" class:completed=(comic.reading.status == ReadingStatus::Completed)>
                        {comic.reading.label(comic.page_count)}
                    </span>
                    {(comic.reading.status != ReadingStatus::Unread).then(|| comic.reading.percent(comic.page_count)).flatten().map(|percent| view! {
                        <progress class="chapter-progress" aria-label="Book reading progress" max="100" value=percent></progress>
                    })}
                </div>
            </div>
            <a class="book-read-action" class:unavailable=!available aria-disabled=(!available).to_string()
                href=href on:click=move |e| if !available { e.prevent_default(); }
                aria-label=format!("{action}: {}", title.get_value())>{action}<span aria-hidden="true">" →"</span></a>
            <button class="icon-button book-more" aria-label=format!("Actions for {}", title.get_value()) on:click=open_menu>"⋯"</button>
        </article>
    }
}

#[component]
pub fn BookList(comics: Memo<Vec<ComicBook>>) -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();
    let selected_count = move || ctx.selected_comics.with(|s| s.len());
    let all_selected = move || {
        comics.with(|books| {
            !books.is_empty()
                && ctx
                    .selected_comics
                    .with(|s| books.iter().all(|b| s.contains(&b.id)))
        })
    };
    view! {
        <div class="book-list-container">
            <Show when=move || comics.with(|books| !books.is_empty())>
                <div class="book-list-tools">
                    <label><input type="checkbox" prop:checked=all_selected aria-label="Select all visible books"
                        on:change=move |e| {
                            let checked = event_target_checked(&e);
                            ctx.selected_comics.update(|selected| {
                                for book in comics.get_untracked() {
                                    if checked { selected.insert(book.id); } else { selected.remove(&book.id); }
                                }
                            });
                        } />"Select all"</label>
                    <Show when=move || { selected_count() > 0 } fallback=|| view! { <span>"In reading order"</span> }>
                        <span>{move || format!("{} selected", selected_count())}</span>
                        <button class="text-button" on:click=move |_| ctx.selected_comics.update(|s| s.clear())>"Clear selection"</button>
                        <button class="btn-secondary" on:click=move |e: leptos::ev::MouseEvent| ctx.context_menu.set(Some(ContextMenuState {
                            x: e.client_x() as f64, y: e.client_y() as f64,
                            target: ContextMenuTarget::MultiSelect { ids: ctx.selected_comics.get_untracked().into_iter().collect() },
                        }))>"Selection actions"</button>
                    </Show>
                </div>
            </Show>
            <Show when=move || comics.with(Vec::is_empty)><super::empty_state::NoResults /></Show>
            <div class="book-list">
                <For each=move || comics.get()
                    key=|c| (c.id.clone(), c.title.clone(), c.series.clone(), c.downloaded, c.page_count, c.reading.clone())
                    children=move |comic| view! { <ComicCard comic=comic /> } />
            </div>
        </div>
    }
}
