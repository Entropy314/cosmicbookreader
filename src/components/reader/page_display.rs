use leptos::prelude::*;

use crate::hooks::use_comic_reader::{FitMode, ReaderState};

#[component]
pub fn PageDisplay(reader: ReaderState) -> impl IntoView {
    let ctx = use_context::<crate::state::AppContext>().unwrap();
    let downloading = move || {
        ctx.drive_status.get().downloading_comic_id.as_deref()
            == Some(reader.comic_id.get_value().as_str())
    };
    let cancel_download = move |_| {
        leptos::task::spawn_local(async move {
            if let Err(error) = crate::invoke::cancel_drive_operation().await {
                let _ = reader.error.try_set(Some(error));
            }
        });
    };
    let img_style = move || {
        match reader.fit_mode.get() {
            FitMode::FitPage => "max-width: 100%; max-height: 100vh; object-fit: contain; display: block; margin: 0 auto;".to_string(),
            FitMode::FitWidth => "width: 100%; height: auto; display: block;".to_string(),
            FitMode::FitHeight => "height: 100vh; width: auto; display: block; margin: 0 auto;".to_string(),
            FitMode::Free => {
                let z = reader.zoom.get();
                format!("transform: scale({z}); transform-origin: top center; display: block; margin: 0 auto;")
            }
        }
    };

    view! {
        <div class="page-display">
            {move || reader.is_loading.get().then(|| view! {
                <div class="page-loading">
                    <div class="spinner"></div>
                    {move || reader.page_data.with(|page| page.is_none()).then(|| view! {
                        <div class="reader-download-status" role="status" aria-live="polite">
                            <p>{move || if downloading() { ctx.drive_status.get().message } else { "Opening comic…".into() }}</p>
                            {move || downloading().then(|| view! {
                                <button class="btn-secondary" on:click=cancel_download>"Cancel download"</button>
                            })}
                            <a href="/">"← Back to Library"</a>
                        </div>
                    })}
                </div>
            })}
            {move || reader.page_data.get().map(|data| view! {
                <img
                    src=data.data_uri
                    alt=format!("Page {}", data.index + 1)
                    style=img_style()
                    class="page-img"
                />
            })}
        </div>
    }
}
