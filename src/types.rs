use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ComicFormat {
    Cbz,
    Cbr,
    Cb7,
    Pdf,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComicBook {
    pub id: String,
    pub path: String,
    pub title: String,
    pub series: String,
    pub format: ComicFormat,
    pub page_count: Option<u32>,
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
