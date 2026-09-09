use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

use crate::cache::CacheManager;
use crate::formats::ComicArchive;
use crate::types::ComicBook;

pub struct ActiveReader {
    pub comic_id: String,
    pub archive: Box<dyn ComicArchive + Send>,
    pub page_count: u32,
}

pub struct AppState {
    pub reader_request: std::sync::atomic::AtomicU64,
    pub drive: crate::drive::DriveService,
    pub cache: Arc<Mutex<CacheManager>>,
    pub library: Arc<RwLock<HashMap<String, ComicBook>>>,
    pub active_reader: Arc<Mutex<Option<ActiveReader>>>,
}

impl AppState {
    pub fn new(cache: CacheManager, comics: Vec<ComicBook>, drive: crate::drive::DriveService) -> Self {
        let library = comics.into_iter().map(|c| (c.id.clone(), c)).collect();
        AppState {
            reader_request: std::sync::atomic::AtomicU64::new(0),
            drive,
            cache: Arc::new(Mutex::new(cache)),
            library: Arc::new(RwLock::new(library)),
            active_reader: Arc::new(Mutex::new(None)),
        }
    }
}
