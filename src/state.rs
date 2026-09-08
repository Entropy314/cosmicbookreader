use std::collections::{HashMap, HashSet};

use leptos::prelude::*;

use crate::types::ComicBook;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Grid,
    Shelves,
}

#[derive(Clone)]
pub enum ContextMenuTarget {
    Comic { id: String },
    Series { ids: Vec<String> },
    MultiSelect { ids: Vec<String> },
}

#[derive(Clone)]
pub struct ContextMenuState {
    pub x: f64,
    pub y: f64,
    pub target: ContextMenuTarget,
}

#[derive(Clone, Copy)]
pub struct AppContext {
    pub library: RwSignal<Vec<ComicBook>>,
    pub current_directory: RwSignal<Option<String>>,
    pub cover_cache: RwSignal<HashMap<String, String>>,
    pub is_loading_library: RwSignal<bool>,
    pub error_message: RwSignal<Option<String>>,
    pub view_mode: RwSignal<ViewMode>,
    pub selected_series: RwSignal<Option<String>>,
    pub context_menu: RwSignal<Option<ContextMenuState>>,
    pub search_query: RwSignal<String>,
    pub selected_comics: RwSignal<HashSet<String>>,
}

impl AppContext {
    pub fn new() -> Self {
        AppContext {
            library: RwSignal::new(Vec::new()),
            current_directory: RwSignal::new(load_saved_directory()),
            cover_cache: RwSignal::new(HashMap::new()),
            is_loading_library: RwSignal::new(false),
            error_message: RwSignal::new(None),
            view_mode: RwSignal::new(ViewMode::Shelves),
            selected_series: RwSignal::new(None),
            context_menu: RwSignal::new(None),
            search_query: RwSignal::new(String::new()),
            selected_comics: RwSignal::new(HashSet::new()),
        }
    }
}

/// Read a persisted preference. Missing or unavailable storage reads as None.
pub fn load_setting(key: &str) -> Option<String> {
    web_sys::window()
        .and_then(|w| w.local_storage().ok().flatten())
        .and_then(|s| s.get_item(key).ok().flatten())
}

/// Persist a preference. Best-effort: settings are conveniences, not data.
pub fn save_setting(key: &str, value: &str) {
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let _ = storage.set_item(key, value);
    }
}

fn load_saved_directory() -> Option<String> {
    load_setting("last_directory")
}

pub fn save_directory(dir: &str) {
    save_setting("last_directory", dir);
}

/// Whether this series is read right-to-left, as manga is.
pub fn load_rtl(series: &str) -> bool {
    load_setting(&format!("rtl:{series}")).as_deref() == Some("1")
}

pub fn save_rtl(series: &str, rtl: bool) {
    save_setting(&format!("rtl:{series}"), if rtl { "1" } else { "0" });
}

/// The neighbouring chapter within the same series, in reading order.
pub fn sibling_chapter(ctx: AppContext, comic_id: &str, offset: isize) -> Option<String> {
    ctx.library.with_untracked(|lib| sibling_in(lib, comic_id, offset))
}

/// The library arrives sorted by (series, title), so the entry either side of
/// this one is the previous or next chapter - provided it is the same series.
fn sibling_in(library: &[ComicBook], comic_id: &str, offset: isize) -> Option<String> {
    let at = library.iter().position(|c| c.id == comic_id)?;
    let sibling = library.get(at.checked_add_signed(offset)?)?;
    (sibling.series == library[at].series).then(|| sibling.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ComicFormat;

    fn comic(series: &str, id: &str) -> ComicBook {
        ComicBook {
            id: id.into(),
            path: String::new(),
            title: id.into(),
            series: series.into(),
            format: ComicFormat::Cbz,
            page_count: None,
        }
    }

    /// Two series, as the backend would hand them over: sorted and adjacent.
    fn library() -> Vec<ComicBook> {
        vec![
            comic("Saga", "s1"),
            comic("Saga", "s2"),
            comic("Saga", "s3"),
            comic("Spy x Family", "x1"),
            comic("Spy x Family", "x2"),
        ]
    }

    #[test]
    fn walks_within_a_series() {
        assert_eq!(sibling_in(&library(), "s2", 1).as_deref(), Some("s3"));
        assert_eq!(sibling_in(&library(), "s2", -1).as_deref(), Some("s1"));
    }

    #[test]
    fn stops_at_series_boundaries() {
        // s3 is the last Saga chapter; the next entry belongs to another series.
        assert_eq!(sibling_in(&library(), "s3", 1), None);
        // x1 is the first of its series.
        assert_eq!(sibling_in(&library(), "x1", -1), None);
    }

    #[test]
    fn stops_at_the_ends_of_the_library() {
        assert_eq!(sibling_in(&library(), "s1", -1), None, "must not underflow");
        assert_eq!(sibling_in(&library(), "x2", 1), None);
    }

    #[test]
    fn unknown_comic_has_no_siblings() {
        assert_eq!(sibling_in(&library(), "nope", 1), None);
        assert_eq!(sibling_in(&[], "s1", 1), None);
    }
}
