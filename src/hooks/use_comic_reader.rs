use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::invoke;
use crate::state::{load_setting, save_setting};
use crate::types::{OpenComicResult, PageData};

#[derive(Clone, Copy, PartialEq)]
pub enum FitMode {
    FitPage,
    FitWidth,
    FitHeight,
    Free,
}

impl FitMode {
    pub fn next(self) -> Self {
        match self {
            FitMode::FitPage => FitMode::FitWidth,
            FitMode::FitWidth => FitMode::FitHeight,
            FitMode::FitHeight => FitMode::Free,
            FitMode::Free => FitMode::FitPage,
        }
    }

    /// Stable key for persisting the choice.
    fn key(self) -> &'static str {
        match self {
            FitMode::FitPage => "page",
            FitMode::FitWidth => "width",
            FitMode::FitHeight => "height",
            FitMode::Free => "free",
        }
    }

    fn from_key(key: &str) -> Self {
        match key {
            "width" => FitMode::FitWidth,
            "height" => FitMode::FitHeight,
            "free" => FitMode::Free,
            _ => FitMode::FitPage,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            FitMode::FitPage => "Fit Page",
            FitMode::FitWidth => "Fit Width",
            FitMode::FitHeight => "Fit Height",
            FitMode::Free => "Free",
        }
    }
}

#[derive(Clone, Copy)]
pub struct ReaderState {
    pub current_page: RwSignal<u32>,
    pub page_count: RwSignal<u32>,
    pub page_data: RwSignal<Option<PageData>>,
    pub is_loading: RwSignal<bool>,
    pub error: RwSignal<Option<String>>,
    pub fit_mode: RwSignal<FitMode>,
    pub zoom: RwSignal<f64>,
    pub toolbar_visible: RwSignal<bool>,
    /// Right-to-left page order, as manga is read.
    pub rtl: RwSignal<bool>,
    pub comic_id: StoredValue<String>,
    /// The next page, fetched ahead of the reader asking for it.
    prefetch: RwSignal<Option<PageData>>,
}

impl ReaderState {
    pub fn next_page(&self) {
        let cur = self.current_page.get_untracked();
        if cur + 1 < self.page_count.get_untracked() {
            self.go_to_page(cur + 1);
        }
    }

    pub fn prev_page(&self) {
        let cur = self.current_page.get_untracked();
        if cur > 0 {
            self.go_to_page(cur - 1);
        }
    }

    pub fn go_to_page(&self, page: u32) {
        self.current_page.set(page);

        // The next page is usually already in hand - show it immediately.
        if let Some(data) = self.prefetch.get_untracked().filter(|d| d.index == page) {
            self.prefetch.set(None);
            self.page_data.set(Some(data));
            self.is_loading.set(false);
            self.record_and_prefetch(page);
            return;
        }

        let id = self.comic_id.get_value();
        let (page_data, is_loading, error) = (self.page_data, self.is_loading, self.error);
        let state = *self;
        is_loading.set(true);

        spawn_local(async move {
            match invoke::get_page(&id, page).await {
                Ok(data) => page_data.set(Some(data)),
                Err(e) => error.set(Some(e)),
            }
            is_loading.set(false);
            state.record_and_prefetch(page);
        });
    }

    /// Save the position, then warm the following page so the next turn is
    /// instant. Reading is overwhelmingly forward, so one page of lookahead
    /// covers almost every turn.
    fn record_and_prefetch(&self, page: u32) {
        let id = self.comic_id.get_value();
        let slot = self.prefetch;
        let next = (page + 1 < self.page_count.get_untracked()).then_some(page + 1);

        spawn_local(async move {
            // Saved on every turn: leaving via Esc, the back button, or a crash
            // should all keep the position.
            let _ = invoke::save_progress(&id, page).await;

            match next {
                Some(next) => {
                    if let Ok(data) = invoke::get_page(&id, next).await {
                        slot.set(Some(data));
                    }
                }
                None => slot.set(None),
            }
        });
    }

    pub fn zoom_in(&self) {
        self.zoom.update(|z| *z = (*z + 0.25).min(4.0));
        self.fit_mode.set(FitMode::Free);
        self.remember_view();
    }

    pub fn zoom_out(&self) {
        self.zoom.update(|z| *z = (*z - 0.25).max(0.25));
        self.fit_mode.set(FitMode::Free);
        self.remember_view();
    }

    pub fn cycle_fit_mode(&self) {
        self.fit_mode.update(|m| *m = m.next());
        self.zoom.set(1.0);
        self.remember_view();
    }

    /// Carry the fit and zoom across chapters - a long series would otherwise
    /// mean setting them again on every one.
    fn remember_view(&self) {
        save_setting("fit_mode", self.fit_mode.get_untracked().key());
        save_setting("zoom", &self.zoom.get_untracked().to_string());
    }

    pub fn toggle_toolbar(&self) {
        self.toolbar_visible.update(|v| *v = !*v);
    }
}

pub fn use_comic_reader(comic_id: String) -> ReaderState {
    let state = ReaderState {
        current_page: RwSignal::new(0),
        page_count: RwSignal::new(0),
        page_data: RwSignal::new(None),
        is_loading: RwSignal::new(true),
        error: RwSignal::new(None),
        fit_mode: RwSignal::new(
            load_setting("fit_mode").map_or(FitMode::FitPage, |k| FitMode::from_key(&k)),
        ),
        zoom: RwSignal::new(
            load_setting("zoom")
                .and_then(|z| z.parse().ok())
                .filter(|z: &f64| (0.25..=4.0).contains(z))
                .unwrap_or(1.0),
        ),
        toolbar_visible: RwSignal::new(true),
        rtl: RwSignal::new(false),
        comic_id: StoredValue::new(comic_id),
        prefetch: RwSignal::new(None),
    };

    // Open comic on mount - spawn directly, no reactive tracking needed
    let id = state.comic_id.get_value();
    spawn_local(async move {
        match invoke::open_comic(&id).await {
            Ok(OpenComicResult { page_count, page, .. }) => {
                let index = page.index;
                state.page_count.set(page_count);
                state.current_page.set(index);
                state.page_data.set(Some(page));
                state.record_and_prefetch(index);
            }
            Err(e) => state.error.set(Some(e)),
        }
        state.is_loading.set(false);
    });

    // Release the archive however the reader is left - back button, Esc, or
    // any other route change.
    let closing_id = state.comic_id.get_value();
    on_cleanup(move || {
        spawn_local(async move {
            let _ = invoke::close_comic(&closing_id).await;
        });
    });

    state
}

#[cfg(test)]
mod tests {
    use super::FitMode;

    #[test]
    fn fit_mode_survives_a_round_trip_through_storage() {
        for mode in [
            FitMode::FitPage,
            FitMode::FitWidth,
            FitMode::FitHeight,
            FitMode::Free,
        ] {
            assert!(FitMode::from_key(mode.key()) == mode, "{}", mode.label());
        }
    }

    #[test]
    fn unknown_fit_mode_key_falls_back_to_fit_page() {
        assert!(FitMode::from_key("") == FitMode::FitPage);
        assert!(FitMode::from_key("nonsense") == FitMode::FitPage);
    }

    #[test]
    fn cycling_fit_modes_returns_to_the_start() {
        let mut mode = FitMode::FitPage;
        for _ in 0..4 {
            mode = mode.next();
        }
        assert!(mode == FitMode::FitPage);
    }
}
