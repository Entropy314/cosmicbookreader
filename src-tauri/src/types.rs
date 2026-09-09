use base64::Engine as _;
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

impl ComicFormat {
    pub fn from_extension(ext: &str) -> Self {
        match ext.to_lowercase().as_str() {
            "cbz" | "zip" => ComicFormat::Cbz,
            "cbr" | "rar" => ComicFormat::Cbr,
            "cb7" | "7z" => ComicFormat::Cb7,
            "pdf" => ComicFormat::Pdf,
            _ => ComicFormat::Unknown,
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComicBook {
    pub id: String,
    pub path: String,
    pub title: String,
    pub series: String,
    pub format: ComicFormat,
    pub page_count: Option<u32>,
    pub cover_cached: bool,
    pub file_size: u64,
    pub modified: u64,
    #[serde(default)]
    pub drive_file_id: Option<String>,
    pub downloaded: bool,
    #[serde(default)]
    pub reading: ReadingProgress,
    #[serde(default)]
    pub series_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

pub fn make_comic_id(path: &str) -> String {
    let hash = blake3::hash(path.as_bytes());
    hash.to_hex()[..16].to_string()
}

pub fn bytes_to_data_uri(bytes: &[u8]) -> String {
    let mime = detect_mime(bytes);
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{};base64,{}", mime, encoded)
}

fn detect_mime(bytes: &[u8]) -> &'static str {
    if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xD8 {
        "image/jpeg"
    } else if bytes.len() >= 8 && &bytes[..8] == b"\x89PNG\r\n\x1a\n" {
        "image/png"
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::series::extract_series;

    #[test]
    fn groups_when_the_chapter_number_leads_the_filename() {
        // Real names from a library where all 84 chapters became 84 series.
        for title in [
            "[MC] [01] Fire Punch @Manga_Campus",
            "[MC] [62] Fire Punch @Manga_Campus",
            // Some of these carry a trailing underscore.
            "[MC] [32] Fire Punch_ @Manga_Campus",
        ] {
            assert_eq!(extract_series(title), "Fire Punch", "title {title:?}");
        }
    }

    #[test]
    fn leading_numbers_that_are_part_of_the_name_survive() {
        // Only bracketed tags lead-strip, never bare numbers.
        assert_eq!(extract_series("2000 AD 1234"), "2000 AD");
        assert_eq!(extract_series("[Group] 2000 AD 1234"), "2000 AD");
    }

    #[test]
    fn groups_filenames_that_use_separators_instead_of_spaces() {
        // Common manga naming: no spaces at all.
        assert_eq!(extract_series("One_Piece_v01"), "One Piece");
        assert_eq!(extract_series("One_Piece_v02"), "One Piece");
        assert_eq!(extract_series("Naruto_c001"), "Naruto");
        assert_eq!(extract_series("Bleach-042"), "Bleach");
        assert_eq!(extract_series("Berserk_Volume_12"), "Berserk");
    }

    #[test]
    fn spaces_win_over_separators_when_both_are_present() {
        // The uploader tag contains an underscore but must stay one token,
        // otherwise stripping stops at its second half.
        assert_eq!(
            extract_series("Solo Leveling - Chapter 000 @Manga_LightN"),
            "Solo Leveling"
        );
        // A hyphenated name keeps its hyphen when the title has spaces.
        assert_eq!(extract_series("Spider-Man 12"), "Spider-Man");
    }

    #[test]
    fn groups_real_world_manga_filenames() {
        // Names taken from an actual library where every file became its own
        // series because the trailing tag stopped the scan immediately.
        for (title, want) in [
            ("Solo Leveling - Chapter 000 @Manga_LightN", "Solo Leveling"),
            ("Solo Leveling - Chapter 142 @Manga_LightN", "Solo Leveling"),
            ("Spy x Family 098.1 (2024) (Digital) (1r0n)", "Spy x Family"),
            ("Spy x Family 120 (2025) (Digital) (1r0n)", "Spy x Family"),
            ("Spy x Family 112.2 (2025) (Digital) (1r0n)", "Spy x Family"),
        ] {
            assert_eq!(extract_series(title), want, "title {title:?}");
        }
    }

    #[test]
    fn a_series_collapses_to_one_name() {
        let titles = [
            "Spy x Family 109 (2025) (Digital) (1r0n)",
            "Spy x Family 110 (2025) (Digital) (1r0n)",
            "Spy x Family 114.1 (2025) (Digital) (1r0n)",
        ];
        let series: std::collections::HashSet<String> =
            titles.iter().map(|t| extract_series(t)).collect();
        assert_eq!(series.len(), 1, "got {series:?}");
    }

    #[test]
    fn strips_chapter_and_volume_keywords() {
        assert_eq!(extract_series("Berserk Volume 12"), "Berserk");
        assert_eq!(extract_series("Naruto Ch. 700"), "Naruto");
        assert_eq!(extract_series("One Piece v01"), "One Piece");
        assert_eq!(extract_series("Bleach c001"), "Bleach");
    }

    #[test]
    fn extract_series_strips_issue_and_year_tokens() {
        assert_eq!(extract_series("Saga 12"), "Saga");
        assert_eq!(extract_series("Saga #12"), "Saga");
        assert_eq!(extract_series("Saga v2 #12 (2019)"), "Saga");
        assert_eq!(extract_series("Batman [2016] 001"), "Batman");
        assert_eq!(extract_series("Y The Last Man 05"), "Y The Last Man");
    }

    #[test]
    fn extract_series_keeps_titles_that_are_not_all_tokens() {
        // Leading numbers are part of the name, not an issue number.
        assert_eq!(extract_series("2000 AD 1234"), "2000 AD");
        assert_eq!(extract_series("Watchmen"), "Watchmen");
        // A title of nothing but tokens keeps its last word rather than
        // collapsing to an empty series.
        assert_eq!(extract_series("12"), "12");
        assert_eq!(extract_series(""), "");
    }

    #[test]
    fn extract_series_ignores_bare_punctuation() {
        // "#" and "v" alone are not issue numbers.
        assert_eq!(extract_series("Hellboy #"), "Hellboy #");
        assert_eq!(extract_series("Hellboy v"), "Hellboy v");
    }

    #[test]
    fn extensions_map_to_formats() {
        for (ext, want) in [
            ("cbz", ComicFormat::Cbz),
            ("zip", ComicFormat::Cbz),
            ("cbr", ComicFormat::Cbr),
            ("rar", ComicFormat::Cbr),
            ("cb7", ComicFormat::Cb7),
            ("7z", ComicFormat::Cb7),
            ("pdf", ComicFormat::Pdf),
            ("txt", ComicFormat::Unknown),
            ("", ComicFormat::Unknown),
        ] {
            assert_eq!(ComicFormat::from_extension(ext), want, "ext {ext:?}");
        }
    }

    #[test]
    fn extension_match_is_case_insensitive() {
        assert_eq!(ComicFormat::from_extension("CBZ"), ComicFormat::Cbz);
        assert_eq!(ComicFormat::from_extension("Cbr"), ComicFormat::Cbr);
    }

    #[test]
    fn data_uri_sniffs_mime_from_magic_bytes() {
        assert!(bytes_to_data_uri(&[0xFF, 0xD8, 0xFF]).starts_with("data:image/jpeg;base64,"));
        assert!(bytes_to_data_uri(b"\x89PNG\r\n\x1a\n").starts_with("data:image/png;base64,"));
    }
}
