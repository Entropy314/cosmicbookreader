use rusqlite::{params, Connection, OptionalExtension};
use super::CacheManager;
use crate::types::{ComicBook, ReadingProgress, ReadingStatus};

pub(super) fn migrate(db: &Connection) -> anyhow::Result<()> {
    let has_status = db.prepare("SELECT 1 FROM pragma_table_info('progress') WHERE name = 'status'")?.exists([])?;
    if !has_status {
        let transaction = db.unchecked_transaction()?;
        transaction.execute_batch(
            "ALTER TABLE progress ADD COLUMN status TEXT NOT NULL DEFAULT 'reading';
             ALTER TABLE progress ADD COLUMN page_count INTEGER;
             UPDATE progress SET page_count = (SELECT page_count FROM library WHERE library.comic_id = progress.comic_id);
             UPDATE progress SET status = 'completed' WHERE page_count > 0 AND page >= page_count - 1;
             UPDATE progress SET updated_at = updated_at * 1000;"
        )?;
        transaction.commit()?;
    }
    Ok(())
}

fn status(value: &str) -> ReadingStatus {
    match value { "reading" => ReadingStatus::Reading, "completed" => ReadingStatus::Completed, _ => ReadingStatus::Unread }
}

fn progress(page: u32, state: &str, updated_at: u64) -> ReadingProgress {
    let state = status(state);
    ReadingProgress { status: state, last_page: (state != ReadingStatus::Unread).then_some(page), updated_at }
}

fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

impl CacheManager {
    pub fn reading_progress(&self, comic_id: &str) -> anyhow::Result<ReadingProgress> {
        Ok(self.db.query_row("SELECT page, status, updated_at FROM progress WHERE comic_id = ?1", [comic_id], |row| {
            Ok(progress(row.get(0)?, &row.get::<_, String>(1)?, row.get(2)?))
        }).optional()?.unwrap_or_default())
    }

    /// Attach progress in one query. Catalog replacement never overwrites it.
    pub fn apply_reading_progress(&self, comics: &mut [ComicBook]) -> anyhow::Result<()> {
        let mut statement = self.db.prepare("SELECT comic_id, page, status, updated_at, page_count FROM progress")?;
        let entries = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, (progress(row.get(1)?, &row.get::<_, String>(2)?, row.get(3)?), row.get::<_, Option<u32>>(4)?)))
        })?.collect::<Result<std::collections::HashMap<_, _>, _>>()?;
        for comic in comics {
            if let Some((reading, count)) = entries.get(&comic.id) {
                comic.reading = reading.clone();
                comic.page_count = comic.page_count.or(*count);
            } else { comic.reading = ReadingProgress::default(); }
        }
        Ok(())
    }

    pub fn record_progress(&self, comic_id: &str, page: u32, page_count: Option<u32>) -> anyhow::Result<ReadingProgress> {
        if let Some(total) = page_count { anyhow::ensure!(page < total, "Cannot save an out-of-range reading position."); }
        let completed = page_count.is_some_and(|total| page + 1 == total);
        // Completion is sticky when revisiting earlier pages. Mark unread is
        // the explicit reset; simply reopening a book never resets its status.
        self.db.execute(
            "INSERT INTO progress (comic_id, page, updated_at, status, page_count) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(comic_id) DO UPDATE SET page = excluded.page, updated_at = excluded.updated_at,
             page_count = COALESCE(excluded.page_count, progress.page_count),
             status = CASE WHEN progress.status = 'completed' OR excluded.status = 'completed' THEN 'completed' ELSE 'reading' END",
            params![comic_id, page, now(), if completed { "completed" } else { "reading" }, page_count],
        )?;
        self.reading_progress(comic_id)
    }

    pub fn set_reading_status(&self, comic_ids: &[String], state: ReadingStatus) -> anyhow::Result<()> {
        anyhow::ensure!(state != ReadingStatus::Reading, "Reading status is set by opening a book.");
        let transaction = self.db.unchecked_transaction()?;
        let (state, updated_at) = match state {
            ReadingStatus::Unread => ("unread", 0),
            _ => ("completed", now()),
        };
        for id in comic_ids {
            transaction.execute(
                "INSERT INTO progress (comic_id, page, updated_at, status) VALUES (?1, 0, ?2, ?3)
                 ON CONFLICT(comic_id) DO UPDATE SET status = excluded.status, updated_at = excluded.updated_at,
                 page = CASE WHEN excluded.status = 'unread' THEN 0 ELSE progress.page END",
                params![id, updated_at, state],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}
