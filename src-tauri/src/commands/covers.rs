use std::path::Path;

use tauri::State;

use crate::cache::extract_thumbnail;
use crate::formats::open_archive;
use crate::state::AppState;
use crate::types::bytes_to_data_uri;

#[tauri::command]
pub async fn get_cover(
    comic_id: String,
    state: State<'_, AppState>,
) -> Result<Option<String>, String> {
    // Get comic info from library
    let comic = {
        let library = state.library.read().await;
        library.get(&comic_id).cloned()
    };

    let comic = match comic {
        Some(c) => c,
        None => return Ok(None),
    };

    let mtime = comic.modified;
    let id = comic_id.clone();

    // Check cache first
    {
        let cache = state.cache.lock().await;
        if let Some(thumb_bytes) = cache.get_thumb(&id, mtime) {
            return Ok(Some(bytes_to_data_uri(&thumb_bytes)));
        }
    }

    // Generate thumbnail in blocking task
    let path = comic.path.clone();
    let id_clone = id.clone();

    let thumb_bytes = tokio::task::spawn_blocking(move || -> anyhow::Result<Vec<u8>> {
        let mut archive = open_archive(Path::new(&path))?;
        extract_thumbnail(archive.as_mut())
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    // Save to cache
    {
        let cache = state.cache.lock().await;
        let _ = cache.save_thumb(
            &id_clone,
            &comic.path,
            mtime,
            &thumb_bytes,
            None,
        );
    }

    Ok(Some(bytes_to_data_uri(&thumb_bytes)))
}

