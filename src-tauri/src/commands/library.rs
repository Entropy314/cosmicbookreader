use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use walkdir::WalkDir;

use crate::state::AppState;
use crate::series::{compare_chapters, folder_hint, regroup, series_for};
use crate::types::{ComicBook, ComicFormat, make_comic_id};

#[tauri::command]
pub async fn pick_directory(app: AppHandle) -> Result<Option<String>, String> {
    let folder = tokio::task::spawn_blocking(move || {
        app.dialog().file().blocking_pick_folder()
    })
    .await
    .map_err(|e| e.to_string())?;

    Ok(folder.map(|f| f.to_string()))
}

#[tauri::command]
pub async fn scan_directory(
    path: String,
    state: State<'_, AppState>,
) -> Result<Vec<ComicBook>, String> {
    let root = PathBuf::from(&path);
    let comics = scan_dir(root.clone(), state.drive.downloads.clone()).await?;
    absorb(&state, &root, comics).await;

    persist(&state).await;
    snapshot(&state).await
}

/// Rescan every folder the library already draws from.
///
/// The stored paths say where the comics live, so new chapters dropped into
/// any known folder are picked up without the user re-picking it - and files
/// deleted on disk drop out.
#[tauri::command]
pub async fn refresh_library(state: State<'_, AppState>) -> Result<Vec<ComicBook>, String> {
    for root in known_roots(&state).await {
        if let Ok(comics) = scan_dir(root.clone(), state.drive.downloads.clone()).await {
            absorb(&state, &root, comics).await;
        }
    }

    persist(&state).await;
    snapshot(&state).await
}

/// Walk one directory for comics, off the async runtime.
async fn scan_dir(root: PathBuf, drive_downloads: PathBuf) -> Result<Vec<ComicBook>, String> {
    tokio::task::spawn_blocking(move || {
        WalkDir::new(&root)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| !e.path().starts_with(&drive_downloads))
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            // Unsupported or unreadable files are skipped silently.
            .filter_map(|e| make_comic_book(e.path()).ok())
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

/// Replace everything under `root` with a fresh scan of it, so files removed
/// from disk also leave the persisted library.
async fn absorb(state: &AppState, root: &Path, comics: Vec<ComicBook>) {
    let mut library = state.library.write().await;
    library.retain(|id, c| crate::drive::is_drive_comic(id) || !Path::new(&c.path).starts_with(root));
    for comic in comics {
        library.insert(comic.id.clone(), comic);
    }
}

/// The distinct folders the library's files live in, with nested folders
/// dropped so a parent scan does not redo its children.
async fn known_roots(state: &AppState) -> Vec<PathBuf> {
    let library = state.library.read().await;
    outermost(
        library
            .values()
            .filter(|c| !crate::drive::is_drive_comic(&c.id))
            .filter_map(|c| Path::new(&c.path).parent().map(Path::to_path_buf))
            .collect(),
    )
}

/// Collapse a list of directories to the outermost ones: duplicates removed,
/// and any folder already covered by an ancestor dropped, since scanning a
/// parent already walks into its children.
fn outermost(mut dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    dirs.sort();
    dirs.dedup();

    // Sorting puts a parent immediately before the children it contains.
    let mut roots: Vec<PathBuf> = Vec::new();
    for dir in dirs {
        if !roots.iter().any(|root| dir.starts_with(root)) {
            roots.push(dir);
        }
    }
    roots
}

#[tauri::command]
pub async fn get_library(state: State<'_, AppState>) -> Result<Vec<ComicBook>, String> {
    snapshot(&state).await
}

#[tauri::command]
pub async fn remove_comic(
    comic_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.library.write().await.remove(&comic_id);
    persist(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn delete_comic_file(
    comic_id: String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let path = {
        let library = state.library.read().await;
        library.get(&comic_id).map(|c| c.path.clone())
    };
    if let Some(path) = path {
        match std::fs::remove_file(&path) {
            Ok(()) => {},
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
            Err(e) => return Err(e.to_string()),
        }
    }
    state.library.write().await.remove(&comic_id);
    persist(&state).await;
    Ok(())
}

#[tauri::command]
pub async fn pick_files(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ComicBook>, String> {
    let picked = tokio::task::spawn_blocking(move || {
        app.dialog().file().blocking_pick_files()
    })
    .await
    .map_err(|e| e.to_string())?;

    let file_paths = picked.unwrap_or_default();
    let mut comics = Vec::new();

    for fp in file_paths {
        let path_string = fp.to_string();
        let path = std::path::Path::new(&path_string);
        if let Ok(comic) = make_comic_book(path) {
            comics.push(comic);
        }
    }

    {
        let mut library = state.library.write().await;
        for comic in comics {
            library.insert(comic.id.clone(), comic);
        }
    }

    persist(&state).await;
    snapshot(&state).await
}

/// Order by series, then chapter/issue number across different filename styles.
pub fn compare_comics(a: &ComicBook, b: &ComicBook) -> std::cmp::Ordering {
    natord::compare_ignore_case(&a.series, &b.series)
        .then_with(|| compare_chapters(&a.title, &b.title))
        .then_with(|| a.id.cmp(&b.id))
}

/// The whole library, sorted for display.
async fn catalog_snapshot(state: &AppState) -> Vec<ComicBook> {
    let mut comics: Vec<ComicBook> = state.library.read().await.values().cloned().collect();
    regroup(&mut comics);
    comics.sort_by(compare_comics);
    comics
}

async fn snapshot(state: &AppState) -> Result<Vec<ComicBook>, String> {
    let cache = state.cache.lock().await;
    let mut comics = catalog_snapshot(state).await;
    cache.apply_reading_progress(&mut comics).map_err(|e| e.to_string())?;
    Ok(comics)
}

/// Write the library to the cache DB. Best-effort: the filesystem is the real
/// source of truth, and a rescan rebuilds whatever fails to persist here.
pub(crate) async fn persist(state: &AppState) {
    if let Err(e) = persist_checked(state).await {
        eprintln!("failed to persist library: {e}");
    }
}

pub(crate) async fn persist_checked(state: &AppState) -> Result<(), String> {
    let cache = state.cache.lock().await;
    let comics = catalog_snapshot(state).await;
    cache.save_comics(&comics).map_err(|e| e.to_string())
}

fn make_comic_book(path: &Path) -> anyhow::Result<ComicBook> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    let format = ComicFormat::from_extension(&ext);
    if matches!(format, ComicFormat::Unknown) {
        anyhow::bail!("Unsupported extension: {}", ext);
    }

    let path_str = path.to_string_lossy().to_string();
    let id = make_comic_id(&path_str);

    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    let folders: Vec<_> = path.parent().into_iter().flat_map(Path::ancestors)
        .take(3).filter_map(|p| p.file_name()?.to_str()).collect();
    let series_hint = folder_hint(folders.into_iter().rev());
    let series = series_for(&title, series_hint.as_deref());

    let meta = std::fs::metadata(path)?;
    let file_size = meta.len();
    let modified = meta
        .modified()
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    Ok(ComicBook {
        id,
        path: path_str,
        title,
        series,
        format,
        page_count: None,
        cover_cached: false,
        file_size,
        modified,
        drive_file_id: None,
        downloaded: true,
        reading: Default::default(),
        series_hint,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn comic(series: &str, title: &str) -> ComicBook {
        ComicBook {
            id: String::new(),
            path: String::new(),
            title: title.into(),
            series: series.into(),
            format: ComicFormat::Cbz,
            page_count: None,
            cover_cached: false,
            file_size: 0,
            modified: 0,
            drive_file_id: None,
            downloaded: true,
            reading: Default::default(),
            series_hint: None,
        }
    }

    fn paths(list: &[&str]) -> Vec<PathBuf> {
        list.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn outermost_keeps_sibling_folders() {
        // The real shape: several unrelated folders under one parent.
        let dirs = paths(&[
            "/Users/x/Downloads/Spy x Family",
            "/Users/x/Downloads/Strike it Rich",
            "/Users/x/Downloads/Spy x Family",
        ]);
        assert_eq!(
            outermost(dirs),
            paths(&["/Users/x/Downloads/Spy x Family", "/Users/x/Downloads/Strike it Rich"])
        );
    }

    #[test]
    fn outermost_drops_folders_a_parent_already_covers() {
        let dirs = paths(&["/a/b/c", "/a", "/a/b"]);
        assert_eq!(outermost(dirs), paths(&["/a"]));
    }

    #[test]
    fn outermost_compares_whole_components() {
        // "/a/bc" is not inside "/a/b" despite the string prefix.
        let dirs = paths(&["/a/b", "/a/bc"]);
        assert_eq!(outermost(dirs), paths(&["/a/b", "/a/bc"]));
    }

    #[test]
    fn issues_sort_numerically_not_lexicographically() {
        let mut comics = vec![
            comic("Saga", "Saga 10"),
            comic("Saga", "Saga 2"),
            comic("Saga", "Saga 1"),
        ];
        comics.sort_by(compare_comics);
        let titles: Vec<&str> = comics.iter().map(|c| c.title.as_str()).collect();
        assert_eq!(titles, ["Saga 1", "Saga 2", "Saga 10"]);
    }

    #[test]
    fn series_orders_before_title() {
        let mut comics = vec![
            comic("Berserk", "Berserk 2"),
            comic("Akira", "Akira 9"),
        ];
        comics.sort_by(compare_comics);
        assert_eq!(comics[0].series, "Akira");
    }

    #[test]
    fn regrouping_merges_filename_variants_and_preserves_distinct_titles() {
        let mut comics = vec![
            comic("old", "CH209 - Chainsaw Man [@The_Mates]"),
            comic("old", "chainsaw man - 002 [Color]"),
            comic("old", "Chainsaw Man - 001 [Color]"),
            comic("old", "berserk chapter a0"),
            comic("old", "Berserk_complete"),
            comic("old", "Solo Leveling 1"),
            comic("old", "Chapter 02 - Solo Leveling_ Ragnarok"),
        ];
        regroup(&mut comics);
        assert!(comics[..3].iter().all(|c| c.series == "Chainsaw Man"));
        assert!(comics[3..5].iter().all(|c| c.series == "Berserk"));
        assert_ne!(comics[5].series, comics[6].series);
        comics.sort_by(compare_comics);
        let chainsaw: Vec<_> = comics.iter().filter(|c| c.series == "Chainsaw Man").map(|c| c.title.as_str()).collect();
        assert_eq!(chainsaw, ["Chainsaw Man - 001 [Color]", "chainsaw man - 002 [Color]", "CH209 - Chainsaw Man [@The_Mates]"]);
        let previous: Vec<_> = comics.iter().map(|c| c.series.clone()).collect();
        regroup(&mut comics);
        assert_eq!(previous, comics.iter().map(|c| c.series.clone()).collect::<Vec<_>>());
    }

    #[test]
    fn unnamed_local_chapters_use_the_series_folder_above_volume_folders() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("Manga/Saga/Volume 01");
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("Chapter 002.cbz");
        std::fs::write(&path, b"book").unwrap();
        let comic = make_comic_book(&path).unwrap();
        assert_eq!(comic.series, "Saga");
        assert_eq!(comic.series_hint.as_deref(), Some("Saga"));
    }

    #[tokio::test]
    async fn library_snapshots_attach_progress_after_catalog_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let cache = crate::cache::CacheManager::new(&directory.path().join("cache")).unwrap();
        let mut book = comic("Saga", "Saga 1");
        book.id = "stable-id".into();
        cache.record_progress(&book.id, 2, Some(3)).unwrap();
        let drive = crate::drive::DriveService::new(&directory.path().join("drive")).unwrap();
        let state = AppState::new(cache, vec![book], drive);
        // New catalog entries start with no progress; persistence must not
        // erase the separate record or deadlock while attaching it.
        persist_checked(&state).await.unwrap();
        let loaded = snapshot(&state).await.unwrap();
        assert_eq!(loaded[0].reading.status, crate::types::ReadingStatus::Completed);
        assert_eq!(loaded[0].reading.last_page, Some(2));
        assert_eq!(loaded[0].page_count, Some(3));
    }
}
