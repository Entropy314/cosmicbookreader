use std::path::Path;

use tauri::State;

use crate::formats::open_archive;
use crate::state::{ActiveReader, AppState};
use crate::types::{OpenComicResult, PageData, ReadingProgress, ReadingStatus, bytes_to_data_uri};

#[tauri::command]
pub async fn open_comic(
    comic_id: String,
    state: State<'_, AppState>,
) -> Result<OpenComicResult, String> {
    let request = state.reader_request.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let comic = {
        let library = state.library.read().await;
        library
            .get(&comic_id)
            .cloned()
            .ok_or_else(|| format!("Comic '{}' not found in library", comic_id))?
    };

    let mut comic = crate::drive::ensure_downloaded(&state, comic).await?;
    let path = comic.path.clone();
    let id = comic_id.clone();

    // Resume where this comic was left off.
    let resume = state.cache.lock().await.get_last_read_page(&comic_id);

    // Open archive and read the resume page in a blocking task
    let (archive, page_count, index, bytes) =
        tokio::task::spawn_blocking(move || -> Result<_, String> {
            let mut archive = open_archive(Path::new(&path))
                .map_err(|e| format!("Failed to open archive: {}", e))?;
            let page_count = archive
                .page_count()
                .map_err(|e| format!("Failed to get page count: {}", e))?;
            // The file may have changed since progress was saved.
            let index = resume.min(page_count.saturating_sub(1));
            let bytes = archive
                .get_page_bytes(index)
                .map_err(|e| format!("Failed to get page {}: {}", index, e))?;
            Ok((archive, page_count, index, bytes))
        })
        .await
        .map_err(|e| e.to_string())??;

    let page = PageData { index, data_uri: bytes_to_data_uri(&bytes) };

    // Update library with page count, and persist it so the next launch
    // already knows how long this comic is.
    {
        let mut library = state.library.write().await;
        if let Some(c) = library.get_mut(&id) {
            c.page_count = Some(page_count);
            c.cover_cached = true;
        }
    }
    super::library::persist(&state).await;

    // Store as active reader
    {
        let mut reader = state.active_reader.lock().await;
        if state.reader_request.load(std::sync::atomic::Ordering::SeqCst) != request {
            return Err("Another comic was opened while this one was loading.".into());
        }
        *reader = Some(ActiveReader { comic_id: id, archive, page_count });
    }

    comic.page_count = Some(page_count);
    comic.reading = state.cache.lock().await.reading_progress(&comic_id).map_err(|e| e.to_string())?;
    Ok(OpenComicResult { comic, page_count, page })
}

#[tauri::command]
pub async fn get_page(
    comic_id: String,
    page_index: u32,
    state: State<'_, AppState>,
) -> Result<PageData, String> {
    let mut reader_guard = state.active_reader.lock().await;
    {
        let reader = reader_guard.as_ref().ok_or("No comic is currently open")?;
        if reader.comic_id != comic_id {
            return Err(format!(
                "Comic '{}' is not the currently open comic",
                comic_id
            ));
        }
        if page_index >= reader.page_count {
            return Err(format!(
                "Page {} out of range (total: {})",
                page_index, reader.page_count
            ));
        }
    }

    // Decompressing and base64-encoding a page is slow - for PDF it is a full
    // page render - so keep it off the async runtime. The reader moves into the
    // blocking task and back; the guard is still held, so access stays serial.
    let active = reader_guard.take().expect("checked above");
    let (active, result) = tokio::task::spawn_blocking(move || {
        let mut active = active;
        let result = active
            .archive
            .get_page_bytes(page_index)
            .map(|bytes| bytes_to_data_uri(&bytes));
        (active, result)
    })
    .await
    .map_err(|e| e.to_string())?;
    *reader_guard = Some(active);

    Ok(PageData {
        index: page_index,
        data_uri: result.map_err(|e| e.to_string())?,
    })
}

#[tauri::command]
pub async fn close_comic(
    comic_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let mut reader = state.active_reader.lock().await;
    // Only clear if this comic is still the open one. Jumping straight to the
    // next chapter opens the new reader before the old one is disposed, and
    // that cleanup must not close the chapter just opened.
    if reader.as_ref().is_some_and(|r| r.comic_id == comic_id) {
        *reader = None;
    }
    Ok(())
}

#[tauri::command]
pub async fn save_progress(
    comic_id: String,
    page: u32,
    state: State<'_, AppState>,
) -> Result<ReadingProgress, String> {
    let count = state.library.read().await.get(&comic_id).and_then(|comic| comic.page_count)
        .ok_or("Open this book before saving its reading position.")?;
    state.cache.lock().await.record_progress(&comic_id, page, Some(count)).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_reading_status(
    comic_ids: Vec<String>,
    status: ReadingStatus,
    state: State<'_, AppState>,
) -> Result<(), String> {
    {
        let library = state.library.read().await;
        if comic_ids.iter().any(|id| !library.contains_key(id)) {
            return Err("One of these books is no longer in the library. Refresh and try again.".into());
        }
    }
    state.cache.lock().await.set_reading_status(&comic_ids, status).map_err(|e| e.to_string())
}

/// Flip the window between fullscreen and windowed, returning the new state.
///
/// Native window fullscreen rather than the webview's element fullscreen, so
/// it behaves like any other desktop app.
#[tauri::command]
pub async fn toggle_fullscreen(window: tauri::Window) -> Result<bool, String> {
    let wanted = !window.is_fullscreen().map_err(|e| e.to_string())?;
    window.set_fullscreen(wanted).map_err(|e| e.to_string())?;
    Ok(wanted)
}
