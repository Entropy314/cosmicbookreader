pub mod thumbnail;

use std::path::{Path, PathBuf};
use rusqlite::{Connection, params};

use crate::types::{extract_series, ComicBook, ComicFormat};

pub use thumbnail::extract_thumbnail;

pub struct CacheManager {
    db: Connection,
    thumb_dir: PathBuf,
}

impl CacheManager {
    pub fn new(cache_dir: &Path) -> anyhow::Result<Self> {
        std::fs::create_dir_all(cache_dir)?;
        let thumb_dir = cache_dir.join("thumbs");
        std::fs::create_dir_all(&thumb_dir)?;

        let db = Connection::open(cache_dir.join("cache.db"))?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS thumbs (
                comic_id    TEXT PRIMARY KEY,
                path        TEXT NOT NULL,
                mtime       INTEGER NOT NULL,
                thumb_path  TEXT NOT NULL,
                page_count  INTEGER,
                created_at  INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS progress (
                comic_id    TEXT PRIMARY KEY,
                page        INTEGER NOT NULL,
                updated_at  INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS library (
                comic_id    TEXT PRIMARY KEY,
                path        TEXT NOT NULL,
                title       TEXT NOT NULL,
                series      TEXT NOT NULL DEFAULT '',
                format      TEXT NOT NULL,
                page_count  INTEGER,
                file_size   INTEGER NOT NULL,
                modified    INTEGER NOT NULL
            );",
        )?;

        // One-time migration: progress used to live in thumbs.last_read_page,
        // which fresh databases no longer have. Without this, upgrading resets
        // every saved reading position to zero.
        let has_legacy_progress = db
            .prepare("SELECT 1 FROM pragma_table_info('thumbs') WHERE name = 'last_read_page'")
            .and_then(|mut stmt| stmt.exists([]))
            .unwrap_or(false);
        if has_legacy_progress {
            db.execute_batch(
                "INSERT OR IGNORE INTO progress (comic_id, page, updated_at)
                 SELECT comic_id, last_read_page, 0 FROM thumbs WHERE last_read_page > 0;",
            )?;
        }

        Ok(CacheManager { db, thumb_dir })
    }

    pub fn get_thumb(&self, comic_id: &str, mtime: u64) -> Option<Vec<u8>> {
        let cached_mtime: i64 = self
            .db
            .query_row(
                "SELECT mtime FROM thumbs WHERE comic_id = ?1",
                params![comic_id],
                |row| row.get(0),
            )
            .ok()?;

        if cached_mtime as u64 != mtime {
            return None;
        }

        // Derive the location rather than trusting the stored thumb_path: an
        // absolute path recorded here goes stale the moment the cache
        // directory moves, which renaming the app does.
        std::fs::read(self.thumb_path(comic_id)).ok()
    }

    fn thumb_path(&self, comic_id: &str) -> PathBuf {
        self.thumb_dir.join(format!("{comic_id}.jpg"))
    }

    pub fn save_thumb(
        &self,
        comic_id: &str,
        path: &str,
        mtime: u64,
        thumb_bytes: &[u8],
        page_count: Option<u32>,
    ) -> anyhow::Result<PathBuf> {
        let thumb_path = self.thumb_path(comic_id);
        std::fs::write(&thumb_path, thumb_bytes)?;

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        self.db.execute(
            "INSERT OR REPLACE INTO thumbs (comic_id, path, mtime, thumb_path, page_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                comic_id,
                path,
                mtime as i64,
                thumb_path.to_string_lossy().to_string(),
                page_count.map(|p| p as i64),
                now as i64
            ],
        )?;

        Ok(thumb_path)
    }

    pub fn save_comics(&self, comics: &[ComicBook]) -> anyhow::Result<()> {
        let mut stmt = self.db.prepare_cached(
            "INSERT OR REPLACE INTO library (comic_id, path, title, series, format, page_count, file_size, modified)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )?;
        for comic in comics {
            stmt.execute(params![
                comic.id,
                comic.path,
                comic.title,
                comic.series,
                comic_format_to_str(&comic.format),
                comic.page_count.map(|p| p as i64),
                comic.file_size as i64,
                comic.modified as i64,
            ])?;
        }
        Ok(())
    }

    pub fn load_comics(&self) -> anyhow::Result<Vec<ComicBook>> {
        let mut stmt = self.db.prepare(
            "SELECT comic_id, path, title, series, format, page_count, file_size, modified FROM library",
        )?;
        let comics = stmt
            .query_map([], |row| {
                let format_str: String = row.get(4)?;
                let page_count: Option<i64> = row.get(5)?;
                let title: String = row.get(2)?;
                Ok(ComicBook {
                    id: row.get(0)?,
                    path: row.get(1)?,
                    // Series is derived from the title, so recompute rather
                    // than trust the stored column. Otherwise entries keep a
                    // stale grouping until their directory happens to be
                    // rescanned, and a library spanning several folders only
                    // ever heals the one that was scanned last.
                    series: extract_series(&title),
                    title,
                    format: str_to_comic_format(&format_str),
                    page_count: page_count.map(|p| p as u32),
                    cover_cached: false,
                    file_size: row.get::<_, i64>(6)? as u64,
                    modified: row.get::<_, i64>(7)? as u64,
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(comics)
    }

    pub fn get_last_read_page(&self, comic_id: &str) -> u32 {
        self.db
            .query_row(
                "SELECT page FROM progress WHERE comic_id = ?1",
                params![comic_id],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as u32
    }

    pub fn save_last_read_page(&self, comic_id: &str, page: u32) -> anyhow::Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Upsert: a comic whose cover was never generated still records progress.
        self.db.execute(
            "INSERT OR REPLACE INTO progress (comic_id, page, updated_at) VALUES (?1, ?2, ?3)",
            params![comic_id, page as i64, now as i64],
        )?;
        Ok(())
    }
}

fn comic_format_to_str(f: &ComicFormat) -> &'static str {
    match f {
        ComicFormat::Cbz => "cbz",
        ComicFormat::Cbr => "cbr",
        ComicFormat::Cb7 => "cb7",
        ComicFormat::Pdf => "pdf",
        ComicFormat::Unknown => "unknown",
    }
}

fn str_to_comic_format(s: &str) -> ComicFormat {
    match s {
        "cbz" => ComicFormat::Cbz,
        "cbr" => ComicFormat::Cbr,
        "cb7" => ComicFormat::Cb7,
        "pdf" => ComicFormat::Pdf,
        _ => ComicFormat::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache(tag: &str) -> CacheManager {
        let dir = std::env::temp_dir().join(format!("kbr-cache-{tag}"));
        std::fs::remove_dir_all(&dir).ok();
        CacheManager::new(&dir).unwrap()
    }

    fn comic(id: &str, title: &str) -> ComicBook {
        ComicBook {
            id: id.into(),
            path: format!("/comics/{title}.cbz"),
            title: title.into(),
            series: "Saga".into(),
            format: ComicFormat::Cbz,
            page_count: Some(20),
            cover_cached: false,
            file_size: 123,
            modified: 456,
        }
    }

    /// Build a database with the pre-migration schema and a saved position.
    fn legacy_db(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kbr-cache-{tag}"));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        let db = Connection::open(dir.join("cache.db")).unwrap();
        db.execute_batch(
            "CREATE TABLE thumbs (
                comic_id TEXT PRIMARY KEY,
                path TEXT NOT NULL,
                mtime INTEGER NOT NULL,
                thumb_path TEXT NOT NULL,
                page_count INTEGER,
                last_read_page INTEGER DEFAULT 0,
                created_at INTEGER NOT NULL
             );
             INSERT INTO thumbs VALUES ('kept', '/a.cbz', 1, '/t.jpg', 30, 17, 0);
             INSERT INTO thumbs VALUES ('unread', '/b.cbz', 1, '/u.jpg', 30, 0, 0);",
        )
        .unwrap();
        dir
    }

    #[test]
    fn upgrading_preserves_positions_from_the_old_schema() {
        let dir = legacy_db("migrate");
        let c = CacheManager::new(&dir).unwrap();
        assert_eq!(c.get_last_read_page("kept"), 17, "position must survive upgrade");
        assert_eq!(c.get_last_read_page("unread"), 0);
    }

    #[test]
    fn migration_does_not_clobber_newer_progress() {
        let dir = legacy_db("migrate-twice");
        let c = CacheManager::new(&dir).unwrap();
        c.save_last_read_page("kept", 25).unwrap();
        drop(c);

        // Reopening must not drag the stale legacy value back over the new one.
        let c = CacheManager::new(&dir).unwrap();
        assert_eq!(c.get_last_read_page("kept"), 25);
    }

    #[test]
    fn progress_saves_without_a_thumbnail_row() {
        // Regression: progress was an UPDATE on `thumbs`, so a comic whose
        // cover had never been generated had no row and silently lost its
        // position while still reporting success.
        let c = cache("progress");
        assert_eq!(c.get_last_read_page("never-thumbed"), 0);
        c.save_last_read_page("never-thumbed", 42).unwrap();
        assert_eq!(c.get_last_read_page("never-thumbed"), 42);
    }

    #[test]
    fn progress_survives_a_library_rescan() {
        let c = cache("progress-rescan");
        c.save_last_read_page("a", 7).unwrap();
        c.save_comics(&[comic("a", "Saga 1")]).unwrap();
        assert_eq!(c.get_last_read_page("a"), 7);
    }

    #[test]
    fn library_round_trips() {
        let c = cache("library");
        c.save_comics(&[comic("a", "Saga 1"), comic("b", "Saga 2")]).unwrap();

        let mut loaded = c.load_comics().unwrap();
        loaded.sort_by(|x, y| x.id.cmp(&y.id));
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].title, "Saga 1");
        assert_eq!(loaded[0].series, "Saga");
        assert_eq!(loaded[0].format, ComicFormat::Cbz);
        assert_eq!(loaded[0].page_count, Some(20));
    }

    #[test]
    fn thumbnails_survive_the_cache_directory_moving() {
        // Regression: thumb_path was stored absolute, so renaming the app (and
        // with it the cache directory) orphaned every cover.
        let from = std::env::temp_dir().join("kbr-cache-move-from");
        let to = std::env::temp_dir().join("kbr-cache-move-to");
        std::fs::remove_dir_all(&from).ok();
        std::fs::remove_dir_all(&to).ok();

        let c = CacheManager::new(&from).unwrap();
        c.save_thumb("a", "/comics/a.cbz", 99, b"jpegbytes", Some(12)).unwrap();
        drop(c);

        std::fs::rename(&from, &to).unwrap();

        let moved = CacheManager::new(&to).unwrap();
        assert_eq!(moved.get_thumb("a", 99).as_deref(), Some(&b"jpegbytes"[..]));
        // A changed file still invalidates the cover.
        assert_eq!(moved.get_thumb("a", 100), None);

        std::fs::remove_dir_all(&to).ok();
    }

    #[test]
    fn stale_series_is_recomputed_on_load() {
        // Regression: series is derived from the title but was persisted, so
        // improving the heuristic left every entry mis-grouped until its own
        // directory was rescanned - and only the last-scanned one ever was.
        let c = cache("stale-series");
        let mut wrong = comic("a", "Spy x Family 109 (2025) (Digital) (1r0n)");
        wrong.series = "Spy x Family 109 (2025) (Digital) (1r0n)".into();
        c.save_comics(&[wrong]).unwrap();

        let loaded = c.load_comics().unwrap();
        assert_eq!(loaded[0].series, "Spy x Family");
    }

    #[test]
    fn rescanning_replaces_rather_than_duplicates() {
        let c = cache("library-replace");
        c.save_comics(&[comic("a", "Saga 1")]).unwrap();
        c.save_comics(&[comic("a", "Saga 1 Remastered")]).unwrap();

        let loaded = c.load_comics().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].title, "Saga 1 Remastered");
    }
}
