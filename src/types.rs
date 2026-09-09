use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveSyncMode {
    #[default]
    IndexOnly,
    Offline,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DriveStatus {
    pub configured: bool,
    pub connected: bool,
    pub folder_id: String,
    pub folder_name: String,
    pub auto_sync: bool,
    pub sync_mode: DriveSyncMode,
    pub last_sync: Option<u64>,
    pub busy: bool,
    pub message: String,
    pub error: Option<String>,
    pub completed: usize,
    pub total: usize,
    pub revision: u64,
    pub downloading_comic_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ComicFormat {
    Cbz,
    Cbr,
    Cb7,
    Pdf,
    Unknown,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingStatus {
    #[default]
    Unread,
    Reading,
    Completed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct ReadingProgress {
    pub status: ReadingStatus,
    /// The last successfully displayed page, indexed from zero.
    pub last_page: Option<u32>,
    /// Unix time in milliseconds; zero for an unread book.
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComicBook {
    pub id: String,
    pub path: String,
    pub title: String,
    pub series: String,
    pub format: ComicFormat,
    pub page_count: Option<u32>,
    pub downloaded: bool,
    #[serde(default)]
    pub reading: ReadingProgress,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PageData {
    pub index: u32,
    pub data_uri: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenComicResult {
    pub comic: ComicBook,
    pub page_count: u32,
    pub page: PageData,
}
