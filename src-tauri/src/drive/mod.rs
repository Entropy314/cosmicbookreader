mod api;
mod auth;
mod client;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};
use tokio::sync::{watch, Mutex, RwLock};

use crate::state::AppState;
use crate::series::{folder_hint, series_for};
use crate::types::{ComicBook, ComicFormat};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncMode {
    #[default]
    IndexOnly,
    Offline,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
struct Config {
    oauth_client_id: String,
    connected: bool,
    folder_id: String,
    folder_name: String,
    auto_sync: bool,
    sync_mode: SyncMode,
    last_sync: Option<u64>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            oauth_client_id: String::new(),
            connected: false,
            folder_id: String::new(),
            folder_name: String::new(),
            auto_sync: true,
            sync_mode: SyncMode::IndexOnly,
            last_sync: None,
        }
    }
}

#[derive(Clone, Default, Serialize)]
pub struct DriveStatus {
    configured: bool,
    connected: bool,
    folder_id: String,
    folder_name: String,
    auto_sync: bool,
    sync_mode: SyncMode,
    last_sync: Option<u64>,
    busy: bool,
    message: String,
    error: Option<String>,
    completed: usize,
    total: usize,
    revision: u64,
    downloading_comic_id: Option<String>,
}

pub struct DriveService {
    pub downloads: PathBuf,
    config_path: PathBuf,
    config: Mutex<Config>,
    status: RwLock<DriveStatus>,
    operation: Mutex<()>,
    cancel: watch::Sender<u64>,
    client: Client,
}

impl DriveService {
    pub fn new(data_dir: &Path) -> anyhow::Result<Self> {
        let downloads = data_dir.join("downloads");
        std::fs::create_dir_all(&downloads)?;
        let config_path = data_dir.join("settings.json");
        let (mut config, error) = match std::fs::read(&config_path) {
            Ok(bytes) => match serde_json::from_slice::<Config>(&bytes) {
                Ok(config) => (config, None),
                Err(_) => (
                    Config::default(),
                    Some("Drive settings could not be read. Reconnect Google Drive.".into()),
                ),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::default(), None),
            Err(e) => return Err(e.into()),
        };
        // Tokens belong to a specific OAuth client. An update that changes the
        // publisher client requires a fresh sign-in, while keeping downloads.
        let publisher = auth::publisher_client();
        if publisher.as_ref().map(|c| c.client_id.as_str()) != Some(config.oauth_client_id.as_str())
        {
            config.connected = false;
        }
        let status = DriveStatus {
            configured: publisher.is_some(),
            connected: config.connected,
            folder_id: config.folder_id.clone(),
            folder_name: config.folder_name.clone(),
            auto_sync: config.auto_sync,
            sync_mode: config.sync_mode,
            last_sync: config.last_sync,
            error,
            ..Default::default()
        };
        Ok(Self {
            downloads,
            config_path,
            config: Mutex::new(config),
            status: RwLock::new(status),
            operation: Mutex::new(()),
            cancel: watch::channel(0).0,
            client: Client::builder()
                .connect_timeout(Duration::from_secs(15))
                .timeout(Duration::from_secs(60))
                .build()?,
        })
    }

    async fn save_config(&self, config: Config) -> Result<(), String> {
        let path = self.config_path.clone();
        let bytes = serde_json::to_vec_pretty(&config).map_err(|e| e.to_string())?;
        tokio::task::spawn_blocking(move || -> Result<(), String> {
            let mut temporary = tempfile::NamedTempFile::new_in(path.parent().unwrap())
                .map_err(|e| e.to_string())?;
            temporary.write_all(&bytes).map_err(|e| e.to_string())?;
            temporary.as_file().sync_all().map_err(|e| e.to_string())?;
            temporary.persist(path).map_err(|e| e.error.to_string())?;
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())??;
        let mut status = self.status.write().await;
        status.connected = config.connected;
        status.folder_id = config.folder_id.clone();
        status.folder_name = config.folder_name.clone();
        status.auto_sync = config.auto_sync;
        status.sync_mode = config.sync_mode;
        status.last_sync = config.last_sync;
        *self.config.lock().await = config;
        Ok(())
    }

    async fn begin(&self, message: &str) {
        let mut status = self.status.write().await;
        status.busy = true;
        status.error = None;
        status.message = message.into();
        status.completed = 0;
        status.total = 0;
        status.downloading_comic_id = None;
    }

    async fn finish(&self, result: &Result<(), String>) {
        let mut status = self.status.write().await;
        status.busy = false;
        status.downloading_comic_id = None;
        status.error = result.as_ref().err().cloned();
        status.message = if result.is_ok() {
            "Up to date".into()
        } else {
            String::new()
        };
    }
}

pub fn is_drive_comic(id: &str) -> bool {
    id.starts_with("drive-")
}

fn comic_from_drive(file: &api::DriveFile, downloads: &Path) -> Result<ComicBook, String> {
    // IDs do not depend on names, parents, or content versions: progress survives
    // renames/moves. Versioned paths avoid overwriting an archive in use.
    let id = format!("drive-{}", blake3::hash(file.id.as_bytes()).to_hex());
    let version: u64 = file
        .version
        .parse()
        .map_err(|_| format!("Google did not return a version for {}.", file.name))?;
    let filename = format!("{}-{}.{}", id, version, file.extension());
    let title = Path::new(&file.name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(&file.name)
        .to_string();
    let series_hint = folder_hint(file.folder_path.iter().map(String::as_str));
    Ok(ComicBook {
        id,
        path: downloads.join(filename).to_string_lossy().into_owned(),
        series: series_for(&title, series_hint.as_deref()),
        title,
        format: ComicFormat::from_extension(&file.extension()),
        page_count: None,
        cover_cached: false,
        file_size: file.file_size()?,
        modified: version,
        drive_file_id: Some(file.id.clone()),
        downloaded: false,
        series_hint,
    })
}

async fn prepare_sync(
    service: &DriveService,
    config: &mut Config,
) -> Result<Vec<ComicBook>, String> {
    let mut session = auth::Session::new(service.client.clone(), auth::load().await?);
    prepare_with_session(service, config, &mut session).await
}

async fn prepare_with_session(
    service: &DriveService,
    config: &mut Config,
    session: &mut auth::Session,
) -> Result<Vec<ComicBook>, String> {
    config.folder_name = api::folder(session, &config.folder_id).await?.name;
    let mut files = api::list_comics(session, &config.folder_id).await?;
    for file in &mut files {
        file.folder_path.insert(0, config.folder_name.clone());
    }
    service.status.write().await.total = files.len();
    let mut comics = Vec::with_capacity(files.len());
    for (index, file) in files.iter().enumerate() {
        service.status.write().await.message = format!(
            "{} {}",
            if config.sync_mode == SyncMode::IndexOnly {
                "Indexing"
            } else {
                "Syncing"
            },
            file.name
        );
        let mut comic = comic_from_drive(file, &service.downloads)?;
        if config.sync_mode == SyncMode::Offline {
            api::download(session, file, Path::new(&comic.path)).await?;
        }
        comic.downloaded = locally_available(&comic).await;
        comics.push(comic);
        service.status.write().await.completed = index + 1;
    }
    Ok(comics)
}

pub async fn locally_available(comic: &ComicBook) -> bool {
    tokio::fs::metadata(&comic.path)
        .await
        .is_ok_and(|m| m.is_file() && m.len() == comic.file_size)
}

async fn download_indexed_comic(
    service: &DriveService,
    session: &mut auth::Session,
    comic: &ComicBook,
) -> Result<ComicBook, String> {
    let remote_id = comic
        .drive_file_id
        .as_deref()
        .ok_or("Sync Google Drive again to refresh this book's index.".to_string())?;
    let remote = api::metadata(session, remote_id).await?;
    if remote.trashed || !remote.is_comic() {
        return Err(
            "This comic is no longer available on Google Drive. Sync your index again.".into(),
        );
    }
    let mut downloaded = comic_from_drive(&remote, &service.downloads)?;
    // The individual metadata endpoint does not include ancestor names.
    downloaded.series_hint = comic.series_hint.clone();
    downloaded.series = series_for(&downloaded.title, downloaded.series_hint.as_deref());
    if downloaded.id != comic.id {
        return Err("The Drive file does not match this comic. Sync your index again.".into());
    }
    api::download(session, &remote, Path::new(&downloaded.path)).await?;
    downloaded.downloaded = true;
    Ok(downloaded)
}

/// Only opening a comic calls this function; catalog and cover requests must
/// never download archives. Cached copies work even after disconnecting.
pub async fn ensure_downloaded(state: &AppState, comic: ComicBook) -> Result<ComicBook, String> {
    if !is_drive_comic(&comic.id) {
        return Ok(comic);
    }
    if locally_available(&comic).await {
        return Ok(ComicBook {
            downloaded: true,
            ..comic
        });
    }
    let service = &state.drive;
    let _guard = service.operation.lock().await;
    // A sync/another open may have completed while waiting for the operation.
    let comic = state
        .library
        .read()
        .await
        .get(&comic.id)
        .cloned()
        .ok_or("This comic has been removed from your library.".to_string())?;
    if locally_available(&comic).await {
        return Ok(ComicBook {
            downloaded: true,
            ..comic
        });
    }
    if !service.config.lock().await.connected {
        return Err(
            "This comic is indexed but has not been downloaded. Connect Google Drive to read it."
                .into(),
        );
    }
    let mut cancellation = service.cancel.subscribe();
    service
        .begin(&format!("Downloading {}…", comic.title))
        .await;
    {
        let mut status = service.status.write().await;
        status.downloading_comic_id = Some(comic.id.clone());
        status.total = 1;
    }
    let prepared = tokio::select! {
        result = async {
            let mut session = auth::Session::new(service.client.clone(), auth::load().await?);
            download_indexed_comic(service, &mut session, &comic).await
        } => result,
        _ = cancellation.changed() => Err("Download cancelled. Open the comic again to retry.".into()),
    };
    let result = async {
        let mut downloaded = prepared?;
        if downloaded.modified == comic.modified {
            downloaded.page_count = comic.page_count;
        }
        let cache = state.cache.lock().await;
        let mut library = state.library.write().await;
        // Removing a book while downloading must not silently add it back.
        if library.contains_key(&downloaded.id) {
            let mut replacement = library.clone();
            replacement.insert(downloaded.id.clone(), downloaded.clone());
            cache
                .save_comics(&replacement.values().cloned().collect::<Vec<_>>())
                .map_err(|e| e.to_string())?;
            *library = replacement;
        }
        drop(library);
        drop(cache);
        let mut status = service.status.write().await;
        status.completed = 1;
        status.revision += 1;
        Ok(downloaded)
    }
    .await;
    service
        .finish(&result.as_ref().map(|_| ()).map_err(Clone::clone))
        .await;
    result
}

fn replace_drive_comics(
    library: &mut std::collections::HashMap<String, ComicBook>,
    comics: Vec<ComicBook>,
) {
    let replacement = comics
        .into_iter()
        .map(|mut comic| {
            if let Some(previous) = library
                .get(&comic.id)
                .filter(|c| c.modified == comic.modified)
            {
                comic.page_count = previous.page_count;
                comic.cover_cached = previous.cover_cached;
            }
            (comic.id.clone(), comic)
        })
        .collect::<Vec<_>>();
    library.retain(|id, _| !is_drive_comic(id));
    library.extend(replacement);
}

async fn sync(state: &AppState) -> Result<(), String> {
    let service = &state.drive;
    let _guard = service
        .operation
        .try_lock()
        .map_err(|_| "A Drive operation is already running.".to_string())?;
    let mut config = service.config.lock().await.clone();
    if !config.connected {
        return Err("Connect Google Drive first.".into());
    }
    let mut cancellation = service.cancel.subscribe();
    service.begin("Checking Drive folders…").await;
    // Cancellation can interrupt network work, but never a library commit.
    let prepared = tokio::select! {
        result = prepare_sync(service, &mut config) => result,
        _ = cancellation.changed() => Err("Sync cancelled. Your existing library is still available.".into()),
    };
    let result = async {
        let comics = prepared?;
        {
            // Match persistence lock order and keep the old in-memory library
            // until the replacement snapshot has committed successfully.
            let cache = state.cache.lock().await;
            let mut library = state.library.write().await;
            let mut replacement = library.clone();
            // Only a complete, successful traversal can remove missing entries.
            replace_drive_comics(&mut replacement, comics);
            cache
                .save_comics(&replacement.values().cloned().collect::<Vec<_>>())
                .map_err(|e| e.to_string())?;
            *library = replacement;
        }
        // Notify the UI even if a subsequent persistence write fails.
        service.status.write().await.revision += 1;
        config.last_sync = Some(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        );
        service.save_config(config).await
    }
    .await;
    service.finish(&result).await;
    result
}

pub fn start_background_sync(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(300));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let state = app.state::<AppState>();
            let enabled = {
                let config = state.drive.config.lock().await;
                config.connected && config.auto_sync
            };
            if enabled {
                let _ = sync(&state).await;
            }
        }
    });
}

#[tauri::command]
pub async fn get_drive_status(state: State<'_, AppState>) -> Result<DriveStatus, String> {
    Ok(state.drive.status.read().await.clone())
}

#[tauri::command]
pub async fn connect_drive(folder: String, state: State<'_, AppState>) -> Result<(), String> {
    let folder_id = api::folder_id(&folder)?;
    let publisher_credentials = auth::connection_credentials()?;
    let service = &state.drive;
    {
        let _guard = service
            .operation
            .try_lock()
            .map_err(|_| "A Drive operation is already running.".to_string())?;
        let mut cancellation = service.cancel.subscribe();
        service
            .begin("Complete Google sign-in in your browser…")
            .await;
        let result = async {
            let (credentials, folder) = tokio::select! {
                result = async {
                    let credentials = auth::authorize(&service.client, publisher_credentials).await?;
                    let mut session = auth::Session::new(service.client.clone(), credentials.clone());
                    let folder = api::folder(&mut session, &folder_id).await?;
                    Ok::<_, String>((credentials, folder))
                } => result?,
                _ = cancellation.changed() => return Err("Google sign-in cancelled.".into()),
            };
            let mut config = service.config.lock().await.clone();
            config.oauth_client_id = credentials.client_id.clone();
            auth::save(credentials).await?;
            config.connected = true;
            config.folder_id = folder_id;
            config.folder_name = folder.name;
            config.last_sync = None;
            service.save_config(config).await
        }.await;
        service.finish(&result).await;
        result?;
    }
    sync(&state).await
}

#[tauri::command]
pub async fn sync_drive(state: State<'_, AppState>) -> Result<(), String> {
    sync(&state).await
}

#[tauri::command]
pub async fn cancel_drive_operation(state: State<'_, AppState>) -> Result<(), String> {
    // Each operation subscribes at its start, so cancellation cannot leak into
    // the next operation and is still observed between network requests.
    state
        .drive
        .cancel
        .send_modify(|generation| *generation = generation.wrapping_add(1));
    Ok(())
}

#[tauri::command]
pub async fn set_drive_auto_sync(enabled: bool, state: State<'_, AppState>) -> Result<(), String> {
    let service = &state.drive;
    let _guard = service
        .operation
        .try_lock()
        .map_err(|_| "Wait for the current Drive operation to finish.".to_string())?;
    let mut config = service.config.lock().await.clone();
    config.auto_sync = enabled;
    service.save_config(config).await
}

#[tauri::command]
pub async fn set_drive_sync_mode(mode: SyncMode, state: State<'_, AppState>) -> Result<(), String> {
    let service = &state.drive;
    let _guard = service.operation.try_lock().map_err(|_| {
        "Cancel or wait for the current Drive operation before changing sync mode.".to_string()
    })?;
    let mut config = service.config.lock().await.clone();
    config.sync_mode = mode;
    service.save_config(config).await
}

#[tauri::command]
pub async fn disconnect_drive(state: State<'_, AppState>) -> Result<(), String> {
    let service = &state.drive;
    let _guard = service
        .operation
        .try_lock()
        .map_err(|_| "Cancel the current Drive operation before disconnecting.".to_string())?;
    // Keep local downloads and reading progress. This forgets the local grant;
    // account-wide revocation is available in Google Account settings.
    auth::forget().await?;
    service.save_config(Config::default()).await?;
    let mut status = service.status.write().await;
    status.error = None;
    status.message.clear();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(id: &str, name: &str, version: &str) -> serde_json::Value {
        serde_json::json!({"id":id,"name":name,"mimeType":"application/zip","version":version,"size":"4"})
    }

    fn folder_metadata() -> String {
        serde_json::json!({"id":"root","name":"Collection","mimeType":"application/vnd.google-apps.folder"}).to_string()
    }

    #[tokio::test]
    async fn index_sync_downloads_no_archives_and_survives_restart() {
        let directory = tempfile::tempdir().unwrap();
        let service = DriveService::new(directory.path()).unwrap();
        let cached_file = remote("cached", "Saga 1.cbz", "1");
        let cloud_file = remote("cloud", "Saga 2.cbz", "1");
        let cached = comic_from_drive(
            &serde_json::from_value(cached_file.clone()).unwrap(),
            &service.downloads,
        )
        .unwrap();
        std::fs::write(&cached.path, b"book").unwrap();
        let (mut session, server) = api::tests::server(vec![
            folder_metadata(),
            serde_json::json!({"files":[cached_file, cloud_file]}).to_string(),
        ])
        .await;
        let mut config = Config {
            folder_id: "root".into(),
            ..Default::default()
        };
        let comics = prepare_with_session(&service, &mut config, &mut session)
            .await
            .unwrap();
        let requests = server.await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|url| !url
            .query_pairs()
            .any(|(key, value)| key == "alt" && value == "media")));
        assert_eq!(comics.len(), 2);
        assert_eq!(std::fs::read_dir(&service.downloads).unwrap().count(), 1);
        let cache = crate::cache::CacheManager::new(&directory.path().join("cache")).unwrap();
        cache.save_comics(&comics).unwrap();
        let restored = cache.load_comics().unwrap();
        assert!(
            restored
                .iter()
                .find(|c| c.drive_file_id.as_deref() == Some("cached"))
                .unwrap()
                .downloaded
        );
        let cloud = restored
            .iter()
            .find(|c| c.drive_file_id.as_deref() == Some("cloud"))
            .unwrap();
        assert!(!cloud.downloaded);
        assert!(!Path::new(&cloud.path).exists());
        let cloud_id = cloud.id.clone();
        let state = AppState::new(cache, restored, service);
        // Rendering a missing cover must succeed without credentials/network.
        assert!(crate::commands::covers::cover_for_comic(cloud_id, &state)
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn opening_downloads_only_the_selected_book_and_reuses_it_disconnected() {
        let directory = tempfile::tempdir().unwrap();
        let service = DriveService::new(directory.path()).unwrap();
        let mut old: api::DriveFile =
            serde_json::from_value(remote("chosen", "Chapter 2.cbz", "1")).unwrap();
        old.folder_path = vec!["Saga".into()];
        let indexed = comic_from_drive(&old, &service.downloads).unwrap();
        let other = comic_from_drive(
            &serde_json::from_value(remote("other", "Saga 1.cbz", "1")).unwrap(),
            &service.downloads,
        )
        .unwrap();
        let latest = remote("chosen", "Chapter 2.cbz", "2").to_string();
        let (mut session, server) =
            api::tests::server(vec![latest.clone(), "book".into(), latest]).await;
        let downloaded = download_indexed_comic(&service, &mut session, &indexed)
            .await
            .unwrap();
        assert_eq!(downloaded.id, indexed.id);
        assert_eq!(downloaded.series, "Saga");
        assert_eq!(downloaded.series_hint, indexed.series_hint);
        assert_eq!(downloaded.modified, 2);
        assert!(downloaded.downloaded);
        assert_eq!(std::fs::read(&downloaded.path).unwrap(), b"book");
        assert!(!Path::new(&other.path).exists());
        let requests = server.await.unwrap();
        assert_eq!(
            requests
                .iter()
                .filter(|u| u.query_pairs().any(|(k, v)| k == "alt" && v == "media"))
                .count(),
            1
        );
        assert!(requests.iter().all(|u| u.path() == "/files/chosen"));
        let cache = crate::cache::CacheManager::new(&directory.path().join("cache")).unwrap();
        cache
            .save_comics(&[downloaded.clone(), other.clone()])
            .unwrap();
        let state = AppState::new(cache, vec![downloaded.clone(), other.clone()], service);
        assert!(!state.drive.config.lock().await.connected);
        // Works offline after disconnect and without reading the credential store.
        let reused = ensure_downloaded(&state, downloaded).await.unwrap();
        assert!(reused.downloaded);
        assert!(ensure_downloaded(&state, other)
            .await
            .unwrap_err()
            .contains("Connect Google Drive"));
    }

    #[tokio::test]
    async fn folder_titles_survive_index_sync_and_restart_without_downloads() {
        let directory = tempfile::tempdir().unwrap();
        let service = DriveService::new(directory.path()).unwrap();
        let folder = |id, name| serde_json::json!({"id":id,"name":name,"mimeType":"application/vnd.google-apps.folder"});
        let (mut session, server) = api::tests::server(vec![
            folder("root", "Solo Leveling Ragnarok").to_string(),
            serde_json::json!({"files":[remote("a", "[Manga Universe] Chapter 06.cbz", "1"), folder("volume", "Volume 01")]}).to_string(),
            serde_json::json!({"files":[remote("b", "Chapter 48 - Solo Leveling.cbz", "1")]}).to_string(),
        ]).await;
        let mut config = Config { folder_id: "root".into(), ..Default::default() };
        let comics = prepare_with_session(&service, &mut config, &mut session).await.unwrap();
        assert_eq!(server.await.unwrap().len(), 3);
        assert_eq!(comics.len(), 2);
        assert_eq!(std::fs::read_dir(&service.downloads).unwrap().count(), 0);
        let cache_path = directory.path().join("cache");
        let cache = crate::cache::CacheManager::new(&cache_path).unwrap();
        cache.save_comics(&comics).unwrap();
        cache.save_last_read_page(&comics[0].id, 7).unwrap();
        drop(cache);
        let cache = crate::cache::CacheManager::new(&cache_path).unwrap();
        let restored = cache.load_comics().unwrap();
        assert!(restored.iter().all(|c| c.series == "Solo Leveling Ragnarok"
            && c.series_hint.as_deref() == Some("Solo Leveling Ragnarok") && !c.downloaded));
        assert_eq!(cache.get_last_read_page(&comics[0].id), 7);
    }

    #[tokio::test]
    async fn offline_mode_still_downloads_the_collection() {
        let directory = tempfile::tempdir().unwrap();
        let service = DriveService::new(directory.path()).unwrap();
        let file = remote("comic", "Saga.cbz", "1");
        let (mut session, server) = api::tests::server(vec![
            folder_metadata(),
            serde_json::json!({"files":[file.clone()]}).to_string(),
            "book".into(),
            file.to_string(),
        ])
        .await;
        let mut config = Config {
            folder_id: "root".into(),
            sync_mode: SyncMode::Offline,
            ..Default::default()
        };
        let comics = prepare_with_session(&service, &mut config, &mut session)
            .await
            .unwrap();
        assert!(comics[0].downloaded);
        assert_eq!(std::fs::read(&comics[0].path).unwrap(), b"book");
        server.await.unwrap();
    }

    #[tokio::test]
    async fn defaults_existing_settings_to_index_only_and_persists_the_choice() {
        let old: Config = serde_json::from_str(r#"{"connected":true,"auto_sync":true}"#).unwrap();
        assert_eq!(old.sync_mode, SyncMode::IndexOnly);
        let directory = tempfile::tempdir().unwrap();
        let service = DriveService::new(directory.path()).unwrap();
        service
            .save_config(Config {
                sync_mode: SyncMode::Offline,
                ..Default::default()
            })
            .await
            .unwrap();
        let restored = DriveService::new(directory.path()).unwrap();
        assert_eq!(restored.status.read().await.sync_mode, SyncMode::Offline);
    }

    #[tokio::test]
    async fn old_settings_cannot_enable_drive_or_reuse_another_clients_connection() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(
            directory.path().join("settings.json"),
            br#"{
            "configured":true, "connected":true,
            "oauth_client_id":"old-client.apps.googleusercontent.com",
            "folder_id":"keep-this-folder", "folder_name":"My collection"
        }"#,
        )
        .unwrap();
        let service = DriveService::new(directory.path()).unwrap();
        let status = service.status.read().await.clone();
        assert_eq!(status.configured, auth::publisher_client().is_some());
        assert!(!status.connected);
        assert_eq!(status.folder_id, "keep-this-folder");
        service.save_config(Config::default()).await.unwrap();
        // Disconnect clears the user's connection, not publisher configuration.
        assert_eq!(
            service.status.read().await.configured,
            auth::publisher_client().is_some()
        );
    }

    #[test]
    fn replacing_drive_catalog_preserves_local_imports_and_unchanged_metadata() {
        let file: api::DriveFile = serde_json::from_value(serde_json::json!({
            "id": "remote", "name": "Saga.cbz", "mimeType": "application/zip", "version": "1", "size": "4"
        })).unwrap();
        let incoming = comic_from_drive(&file, Path::new("/downloads")).unwrap();
        let mut previous = incoming.clone();
        previous.page_count = Some(24);
        let mut local = incoming.clone();
        local.id = "local-book".into();
        let mut removed = incoming.clone();
        removed.id = "drive-removed".into();
        let mut library = [previous, local, removed]
            .into_iter()
            .map(|c| (c.id.clone(), c))
            .collect();
        replace_drive_comics(&mut library, vec![incoming.clone()]);
        assert_eq!(library.len(), 2);
        assert!(library.contains_key("local-book"));
        assert!(!library.contains_key("drive-removed"));
        assert_eq!(library[&incoming.id].page_count, Some(24));
        let mut updated = incoming.clone();
        updated.modified = 2;
        replace_drive_comics(&mut library, vec![updated]);
        assert_eq!(library[&incoming.id].page_count, None);
        replace_drive_comics(&mut library, vec![]);
        assert_eq!(library.len(), 1);
        assert!(library.contains_key("local-book"));
    }

    #[test]
    fn drive_identity_survives_renames_and_revisions_with_safe_local_paths() {
        let mut file: api::DriveFile = serde_json::from_value(serde_json::json!({
            "id": "remote-id", "name": "Saga 01.cbz", "mimeType": "application/zip", "version": "1", "size": "123"
        })).unwrap();
        let root = Path::new("/downloads");
        let first = comic_from_drive(&file, root).unwrap();
        file.name = "../../Renamed 01.cbz".into();
        file.version = "2".into();
        let updated = comic_from_drive(&file, root).unwrap();
        assert_eq!(first.id, updated.id);
        assert_ne!(first.path, updated.path);
        assert_eq!(Path::new(&updated.path).parent(), Some(root));
        assert_eq!(updated.title, "Renamed 01");
        assert_eq!(updated.series, "Renamed");
    }
}
