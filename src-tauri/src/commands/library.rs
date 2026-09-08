use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use walkdir::WalkDir;

use crate::state::AppState;
use crate::types::{extract_series, ComicBook, ComicFormat, make_comic_id};

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
    let comics = scan_dir(root.clone()).await?;
    absorb(&state, &root, comics).await;

    persist(&state).await;
    Ok(snapshot(&state).await)
}

/// Rescan every folder the library already draws from.
///
/// The stored paths say where the comics live, so new chapters dropped into
/// any known folder are picked up without the user re-picking it - and files
/// deleted on disk drop out.
#[tauri::command]
pub async fn refresh_library(state: State<'_, AppState>) -> Result<Vec<ComicBook>, String> {
    for root in known_roots(&state).await {
        if let Ok(comics) = scan_dir(root.clone()).await {
            absorb(&state, &root, comics).await;
        }
    }

    persist(&state).await;
    Ok(snapshot(&state).await)
}

/// Walk one directory for comics, off the async runtime.
async fn scan_dir(root: PathBuf) -> Result<Vec<ComicBook>, String> {
    tokio::task::spawn_blocking(move || {
        WalkDir::new(&root)
            .follow_links(true)
            .into_iter()
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
    library.retain(|_, c| !Path::new(&c.path).starts_with(root));
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
    Ok(snapshot(&state).await)
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
        std::fs::remove_file(&path).map_err(|e| e.to_string())?;
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
    Ok(snapshot(&state).await)
}

/// Order by series, then title, using natural (issue-aware) ordering so
/// issue 2 sorts before issue 10.
pub fn compare_comics(a: &ComicBook, b: &ComicBook) -> std::cmp::Ordering {
    natord::compare_ignore_case(&a.series, &b.series)
        .then_with(|| natord::compare_ignore_case(&a.title, &b.title))
}

/// The whole library, sorted for display.
async fn snapshot(state: &AppState) -> Vec<ComicBook> {
    let mut comics: Vec<ComicBook> = state.library.read().await.values().cloned().collect();
    comics.sort_by(compare_comics);
    comics
}

/// Write the library to the cache DB. Best-effort: the filesystem is the real
/// source of truth, and a rescan rebuilds whatever fails to persist here.
pub(crate) async fn persist(state: &AppState) {
    let comics = snapshot(state).await;
    let cache = state.cache.lock().await;
    if let Err(e) = cache.save_comics(&comics) {
        eprintln!("failed to persist library: {e}");
    }
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

    let series = extract_series(&title);

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
}
