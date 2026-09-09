use super::CacheManager;
use crate::types::{ComicBook, ReadingProgress, ReadingStatus};
use rusqlite::{params, Connection, OptionalExtension};

pub(super) fn migrate(db: &Connection) -> anyhow::Result<()> {
    let has_status = db
        .prepare("SELECT 1 FROM pragma_table_info('progress') WHERE name = 'status'")?
        .exists([])?;
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
    match value {
        "reading" => ReadingStatus::Reading,
        "completed" => ReadingStatus::Completed,
        _ => ReadingStatus::Unread,
    }
}

fn progress(page: u32, state: &str, updated_at: u64) -> ReadingProgress {
    let state = status(state);
    ReadingProgress {
        status: state,
        last_page: (state != ReadingStatus::Unread).then_some(page),
        updated_at,
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl CacheManager {
    pub fn reading_progress(&self, comic_id: &str) -> anyhow::Result<ReadingProgress> {
        Ok(self
            .db
            .query_row(
                "SELECT page, status, updated_at FROM progress WHERE comic_id = ?1",
                [comic_id],
                |row| {
                    Ok(progress(
                        row.get(0)?,
                        &row.get::<_, String>(1)?,
                        row.get(2)?,
                    ))
                },
            )
            .optional()?
            .unwrap_or_default())
    }

    /// Attach progress in one query. Catalog replacement never overwrites it.
    pub fn apply_reading_progress(&self, comics: &mut [ComicBook]) -> anyhow::Result<()> {
        let mut statement = self
            .db
            .prepare("SELECT comic_id, page, status, updated_at, page_count FROM progress")?;
        let entries = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    (
                        progress(row.get(1)?, &row.get::<_, String>(2)?, row.get(3)?),
                        row.get::<_, Option<u32>>(4)?,
                    ),
                ))
            })?
            .collect::<Result<std::collections::HashMap<_, _>, _>>()?;
        for comic in comics {
            if let Some((reading, count)) = entries.get(&comic.id) {
                comic.reading = reading.clone();
                comic.page_count = comic.page_count.or(*count);
            } else {
                comic.reading = ReadingProgress::default();
            }
        }
        Ok(())
    }

    pub fn record_progress(
        &self,
        comic_id: &str,
        page: u32,
        page_count: Option<u32>,
    ) -> anyhow::Result<ReadingProgress> {
        if let Some(total) = page_count {
            anyhow::ensure!(
                page < total,
                "Cannot save an out-of-range reading position."
            );
        }
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

    pub fn set_reading_status(
        &self,
        comic_ids: &[String],
        state: ReadingStatus,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            state != ReadingStatus::Reading,
            "Reading status is set by opening a book."
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ComicFormat;

    fn book(id: &str, page_count: Option<u32>) -> ComicBook {
        ComicBook {
            id: id.into(),
            title: "Saga 1".into(),
            series: "Saga".into(),
            path: "/not-downloaded.cbz".into(),
            format: ComicFormat::Cbz,
            page_count,
            cover_cached: false,
            file_size: 10,
            modified: 1,
            drive_file_id: Some(id.into()),
            downloaded: false,
            reading: Default::default(),
            series_hint: None,
        }
    }

    #[test]
    fn first_page_is_reading_and_completion_survives_revisiting_earlier_pages() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CacheManager::new(dir.path()).unwrap();
        assert_eq!(
            cache.reading_progress("a").unwrap().status,
            ReadingStatus::Unread
        );
        let first = cache.record_progress("a", 0, Some(3)).unwrap();
        assert_eq!(first.status, ReadingStatus::Reading);
        assert_eq!(first.last_page, Some(0));
        assert!(cache.record_progress("a", 3, Some(3)).is_err());
        assert_eq!(cache.reading_progress("a").unwrap(), first);
        assert_eq!(
            cache.record_progress("a", 2, Some(3)).unwrap().status,
            ReadingStatus::Completed
        );
        assert_eq!(
            cache.record_progress("a", 1, Some(3)).unwrap().status,
            ReadingStatus::Completed
        );
        assert_eq!(cache.get_last_read_page("a"), 1);
        cache
            .set_reading_status(&["a".into()], ReadingStatus::Unread)
            .unwrap();
        assert_eq!(
            cache.reading_progress("a").unwrap(),
            ReadingProgress::default()
        );
        assert_eq!(cache.get_last_read_page("a"), 0);
        assert_eq!(
            cache.record_progress("a", 0, Some(3)).unwrap().status,
            ReadingStatus::Reading
        );
    }

    #[test]
    fn one_page_books_complete_and_empty_books_cannot_advance() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CacheManager::new(dir.path()).unwrap();
        assert_eq!(
            cache.record_progress("one", 0, Some(1)).unwrap().status,
            ReadingStatus::Completed
        );
        assert!(cache.record_progress("empty", 0, Some(0)).is_err());
        assert_eq!(
            cache.reading_progress("empty").unwrap().status,
            ReadingStatus::Unread
        );
    }

    #[test]
    fn legacy_positions_migrate_without_treating_page_zero_as_unread() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CacheManager::new(dir.path()).unwrap();
        cache
            .save_comics(&[
                book("zero", Some(4)),
                book("last", Some(4)),
                book("unknown", None),
                book("unopened", Some(4)),
            ])
            .unwrap();
        cache.db.execute_batch("DROP TABLE progress; CREATE TABLE progress (comic_id TEXT PRIMARY KEY, page INTEGER NOT NULL, updated_at INTEGER NOT NULL);
            INSERT INTO progress VALUES ('zero', 0, 1700000000), ('last', 3, 1700000001), ('unknown', 8, 1700000002);").unwrap();
        drop(cache);
        let cache = CacheManager::new(dir.path()).unwrap();
        assert_eq!(
            cache.reading_progress("zero").unwrap().status,
            ReadingStatus::Reading
        );
        assert_eq!(
            cache.reading_progress("zero").unwrap().updated_at,
            1700000000000
        );
        assert_eq!(
            cache.reading_progress("last").unwrap().status,
            ReadingStatus::Completed
        );
        assert_eq!(
            cache.reading_progress("unknown").unwrap().status,
            ReadingStatus::Reading
        );
        assert_eq!(
            cache.reading_progress("unopened").unwrap().status,
            ReadingStatus::Unread
        );
        cache
            .set_reading_status(&["last".into()], ReadingStatus::Unread)
            .unwrap();
        drop(cache);
        let cache = CacheManager::new(dir.path()).unwrap();
        assert_eq!(
            cache.reading_progress("last").unwrap().status,
            ReadingStatus::Unread
        );
        assert_eq!(cache.get_last_read_page("unknown"), 8);
    }

    #[test]
    fn catalog_replacement_removal_and_restart_preserve_progress_and_known_page_count() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CacheManager::new(dir.path()).unwrap();
        cache.save_comics(&[book("a", Some(10))]).unwrap();
        cache.record_progress("a", 4, Some(10)).unwrap();
        cache.save_comics(&[]).unwrap();
        let mut renamed = book("a", None);
        renamed.title = "Saga Chapter 001 renamed".into();
        cache.save_comics(&[renamed]).unwrap();
        drop(cache);
        let cache = CacheManager::new(dir.path()).unwrap();
        let restored = cache.load_comics().unwrap().remove(0);
        assert_eq!(restored.reading.status, ReadingStatus::Reading);
        assert_eq!(restored.reading.last_page, Some(4));
        assert_eq!(restored.page_count, Some(10));
        assert!(!restored.downloaded);
    }

    #[test]
    fn cloud_books_can_be_marked_completed_in_bulk_without_archives() {
        let dir = tempfile::tempdir().unwrap();
        let cache = CacheManager::new(dir.path()).unwrap();
        cache
            .save_comics(&[book("drive-a", None), book("drive-b", None)])
            .unwrap();
        let ids = vec!["drive-a".into(), "drive-b".into()];
        cache
            .set_reading_status(&ids, ReadingStatus::Completed)
            .unwrap();
        assert!(cache
            .load_comics()
            .unwrap()
            .iter()
            .all(|b| b.reading.status == ReadingStatus::Completed
                && b.page_count.is_none()
                && !b.downloaded));
        assert!(cache
            .set_reading_status(&ids, ReadingStatus::Reading)
            .is_err());
        assert_eq!(
            cache.reading_progress("drive-a").unwrap().status,
            ReadingStatus::Completed
        );
        cache
            .set_reading_status(&ids, ReadingStatus::Unread)
            .unwrap();
        assert!(cache
            .load_comics()
            .unwrap()
            .iter()
            .all(|b| b.reading.status == ReadingStatus::Unread));
        assert_eq!(
            std::fs::read_dir(dir.path().join("thumbs"))
                .unwrap()
                .count(),
            0
        );
    }
}
