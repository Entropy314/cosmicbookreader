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
    pub cache: Arc<Mutex<CacheManager>>,
    pub library: Arc<RwLock<HashMap<String, ComicBook>>>,
    pub active_reader: Arc<Mutex<Option<ActiveReader>>>,
}

impl AppState {
    pub fn new(cache: CacheManager, comics: Vec<ComicBook>) -> Self {
        let library = comics.into_iter().map(|c| (c.id.clone(), c)).collect();
        AppState {
            cache: Arc::new(Mutex::new(cache)),
            library: Arc::new(RwLock::new(library)),
            active_reader: Arc::new(Mutex::new(None)),
        }
    }
}
