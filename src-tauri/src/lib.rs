mod cache;
mod commands;
mod drive;
mod formats;
mod series;
mod state;
mod types;

use cache::CacheManager;
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let cache_dir = app
                .path()
                .app_cache_dir()
                .expect("Failed to get app cache dir")
                .join("cosmicbookreader");

            let cache = CacheManager::new(&cache_dir)
                .expect("Failed to initialize cache");

            // Restore the library persisted by the last session, so startup
            // paints immediately instead of walking the disk.
            let comics = cache.load_comics().unwrap_or_else(|e| {
                eprintln!("failed to load persisted library: {e}");
                Vec::new()
            });

            let drive_dir = app.path().app_data_dir()?.join("google-drive");
            let drive = drive::DriveService::new(&drive_dir)?;
            app.manage(AppState::new(cache, comics, drive));
            drive::start_background_sync(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            drive::get_drive_status,
            drive::connect_drive,
            drive::sync_drive,
            drive::cancel_drive_operation,
            drive::set_drive_auto_sync,
            drive::set_drive_sync_mode,
            drive::disconnect_drive,
            commands::library::pick_directory,
            commands::library::scan_directory,
            commands::library::get_library,
            commands::library::refresh_library,
            commands::library::remove_comic,
            commands::library::delete_comic_file,
            commands::library::pick_files,
            commands::covers::get_cover,
            commands::reader::open_comic,
            commands::reader::get_page,
            commands::reader::close_comic,
            commands::reader::save_progress,
            commands::reader::toggle_fullscreen,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
