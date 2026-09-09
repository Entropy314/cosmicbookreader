use leptos::prelude::*;
use leptos::task::spawn_local;

use crate::types::DriveSyncMode;
use crate::{invoke, state::AppContext};

#[component]
pub fn DrivePanel() -> impl IntoView {
    let ctx = use_context::<AppContext>().unwrap();
    let folder = RwSignal::new(ctx.drive_status.get_untracked().folder_id);
    let pending = RwSignal::new(false);
    let error = RwSignal::new(None::<String>);
    let busy = move || pending.get() || ctx.drive_status.get().busy;
    let configured = Memo::new(move |_| ctx.drive_status.get().configured);
    let connected = Memo::new(move |_| ctx.drive_status.get().connected);
    let change_mode = move |event| {
        let mode = if event_target_value(&event) == "offline" {
            DriveSyncMode::Offline
        } else {
            DriveSyncMode::IndexOnly
        };
        pending.set(true);
        error.set(None);
        spawn_local(async move {
            let result = invoke::set_drive_sync_mode(mode).await;
            let _ = error.try_set(result.err());
            invoke::refresh_drive_status(ctx).await;
            let _ = pending.try_set(false);
        });
    };
    let connect = move |_| {
        let selected = folder.get_untracked();
        pending.set(true);
        error.set(None);
        spawn_local(async move {
            let result = invoke::connect_drive(&selected).await;
            let _ = error.try_set(result.err());
            invoke::refresh_drive_status(ctx).await;
            let _ = pending.try_set(false);
        });
    };
    let sync = move |_| {
        pending.set(true);
        error.set(None);
        spawn_local(async move {
            let result = invoke::sync_drive().await;
            let _ = error.try_set(result.err());
            invoke::refresh_drive_status(ctx).await;
            let _ = pending.try_set(false);
        });
    };
    let disconnect = move |_| {
        pending.set(true);
        error.set(None);
        spawn_local(async move {
            let result = invoke::disconnect_drive().await;
            let _ = error.try_set(result.err());
            invoke::refresh_drive_status(ctx).await;
            let _ = pending.try_set(false);
        });
    };
    let cancel = move |_| {
        spawn_local(async move {
            if let Err(e) = invoke::cancel_drive_operation().await {
                let _ = error.try_set(Some(e));
            }
        });
    };
    let toggle_auto = move |event| {
        let enabled = event_target_checked(&event);
        pending.set(true);
        error.set(None);
        spawn_local(async move {
            let result = invoke::set_drive_auto_sync(enabled).await;
            let _ = error.try_set(result.err());
            invoke::refresh_drive_status(ctx).await;
            let _ = pending.try_set(false);
        });
    };

    view! {
        <section class="drive-panel" id="drive-panel" aria-labelledby="drive-heading">
            <div class="drive-heading-row">
                <div><p class="eyebrow">"YOUR CONNECTED COLLECTION"</p><h2 id="drive-heading">"Google Drive"</h2></div>
                <button class="icon-button" aria-label="Close Google Drive settings" on:click=move |_| ctx.drive_panel_open.set(false)>"×"</button>
            </div>
            <Show when=move || !configured.get()>
                <p class="drive-setup">"Google Drive is not available in this build. Contact the app publisher for a version with Drive enabled."</p>
            </Show>
            <Show when=move || connected.get()>
                <div class="drive-collection-summary">
                    <div><strong>{move || ctx.drive_status.get().folder_name}</strong>
                        <p>{move || if ctx.drive_status.get().sync_mode == DriveSyncMode::IndexOnly {
                            "Books download as you read. Your catalog stays in sync."
                        } else { "All books download for offline reading." }}</p>
                        <p class="drive-note">{move || ctx.drive_status.get().last_sync.map(|seconds| {
                            let date = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(seconds as f64 * 1000.0));
                            format!("Last synced {}", date.to_locale_string("default", &wasm_bindgen::JsValue::UNDEFINED).as_string().unwrap_or_default())
                        }).unwrap_or_else(|| "Ready for your first sync".into())}</p>
                    </div>
                    <button class="btn-primary" disabled=busy on:click=sync>"Sync now"</button>
                </div>
            </Show>
            <div class="drive-status" role="status" aria-live="polite">
                <Show when=move || pending.get() && !ctx.drive_status.get().busy><p>"Working…"</p></Show>
                {move || ctx.drive_status.get().busy.then(|| view! {
                    <div class="drive-progress-row">
                        <span>{move || ctx.drive_status.get().message}</span>
                        <button class="btn-secondary" on:click=cancel>"Cancel"</button>
                    </div>
                    {move || {
                        let status = ctx.drive_status.get();
                        (status.total > 0).then(|| view! {
                            <progress aria-label="Sync progress" max=status.total value=status.completed></progress>
                            <span class="drive-note">{format!("{} of {} books ready", status.completed, status.total)}</span>
                        })
                    }}
                })}
            </div>
            {move || error.get().or_else(|| ctx.drive_status.get().error).map(|message| view! {
                <p class="drive-error" role="alert">{message}</p>
            })}
            <Show when=move || configured.get()>
                <details class="drive-settings" open=move || !connected.get()>
                    <summary>{move || if connected.get() { "Sync settings & collection folder" } else { "Connect your collection" }}</summary>
                    <div class="drive-folder-row">
                        <label for="drive-folder">"Google Drive folder link"</label>
                        <div class="drive-folder-controls">
                            <input id="drive-folder" type="text" placeholder="Paste a Google Drive folder link"
                                autocomplete="off" spellcheck="false" disabled=busy aria-describedby="drive-folder-help"
                                prop:value=folder on:input=move |e| folder.set(event_target_value(&e)) />
                            <button class="btn-primary" disabled=move || busy() || folder.get().trim().is_empty() on:click=connect>
                                {move || if connected.get() { "Connect this folder" } else { "Sign in with Google" }}
                            </button>
                        </div>
                        <p class="drive-note" id="drive-folder-help">"In Google Drive, open your collection folder and copy its address. Sign-in opens in your browser. Subfolders are included."</p>
                    </div>
                    <div class="drive-mode-row">
                        <label for="drive-sync-mode">"When should books download?"</label>
                        <select id="drive-sync-mode" disabled=busy aria-describedby="drive-mode-help"
                            prop:value=move || if ctx.drive_status.get().sync_mode == DriveSyncMode::Offline { "offline" } else { "index_only" }
                            on:change=change_mode>
                            <option value="index_only">"As I read · Index only (recommended)"</option>
                            <option value="offline">"Download the whole collection"</option>
                        </select>
                        <p class="drive-note" id="drive-mode-help">{move || if ctx.drive_status.get().sync_mode == DriveSyncMode::IndexOnly {
                            "Save space: sync the book list, then download a book when you open it. Downloaded books stay available offline."
                        } else { "The next sync downloads every book. Your computer needs enough space for the full collection." }}</p>
                    </div>
                    <Show when=move || connected.get()>
                        <div class="drive-actions">
                            <label class="drive-auto"><input type="checkbox" prop:checked=move || ctx.drive_status.get().auto_sync disabled=busy on:change=toggle_auto />"Keep my collection up to date"</label>
                            <p class="drive-note">"Sync on launch and every 5 minutes while the app is open."</p>
                        </div>
                        <div class="drive-disconnect">
                            <button class="text-button" disabled=busy on:click=disconnect>"Disconnect Drive"</button>
                            <span class="drive-note">"Keeps downloaded books and reading progress."</span>
                        </div>
                    </Show>
                    <p class="drive-note">"Drive access is read-only. Changes here never delete books from Google Drive."</p>
                </details>
            </Show>
        </section>
    }
}
