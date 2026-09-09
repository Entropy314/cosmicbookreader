use crate::types::{ComicBook, ReadingProgress, ReadingStatus};

impl ReadingProgress {
    pub fn label(&self, page_count: Option<u32>) -> String {
        match self.status {
            ReadingStatus::Unread => "Unread".into(),
            ReadingStatus::Completed => "Completed".into(),
            ReadingStatus::Reading => match (self.last_page, page_count.filter(|n| *n > 0)) {
                (Some(page), Some(total)) => format!(
                    "Reading · page {} of {total}",
                    page.saturating_add(1).min(total)
                ),
                (Some(page), None) => format!("Reading · page {}", page.saturating_add(1)),
                _ => "Reading".into(),
            },
        }
    }

    pub fn percent(&self, page_count: Option<u32>) -> Option<u32> {
        match self.status {
            ReadingStatus::Unread => Some(0),
            ReadingStatus::Completed => Some(100),
            ReadingStatus::Reading => {
                let total = page_count.filter(|n| *n > 0)? as u64;
                Some((((self.last_page? as u64 + 1) * 100 / total).min(99)) as u32)
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SeriesProgress {
    pub total: usize,
    pub completed: usize,
    pub reading: usize,
}

impl SeriesProgress {
    pub fn from_books(books: &[ComicBook]) -> Self {
        Self {
            total: books.len(),
            completed: books
                .iter()
                .filter(|b| b.reading.status == ReadingStatus::Completed)
                .count(),
            reading: books
                .iter()
                .filter(|b| b.reading.status == ReadingStatus::Reading)
                .count(),
        }
    }

    pub fn label(self) -> String {
        let mut label = format!("{} of {} completed", self.completed, self.total);
        if self.reading > 0 {
            label.push_str(&format!(" · {} in progress", self.reading));
        }
        label
    }
}

fn available(book: &&ComicBook) -> bool {
    book.downloaded || book.id.starts_with("drive-")
}

/// Call with the visible, ordered collection so actions respect filters and
/// never leave the selected series. Continue unfinished reading before starting
/// another book; explicit "next unread" skips only completed/started books.
pub fn next_unread(books: &[ComicBook]) -> Option<&ComicBook> {
    books
        .iter()
        .filter(available)
        .find(|b| b.reading.status == ReadingStatus::Unread)
}

pub fn reading_target(books: &[ComicBook]) -> Option<&ComicBook> {
    books
        .iter()
        .filter(available)
        .filter(|b| b.reading.status == ReadingStatus::Reading)
        .max_by_key(|b| b.reading.updated_at)
        .or_else(|| next_unread(books))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ComicFormat;

    fn book(id: &str, status: ReadingStatus, updated_at: u64) -> ComicBook {
        ComicBook {
            id: id.into(),
            title: id.into(),
            series: "Saga".into(),
            path: String::new(),
            format: ComicFormat::Cbz,
            page_count: Some(10),
            downloaded: true,
            reading: ReadingProgress {
                status,
                last_page: (status != ReadingStatus::Unread).then_some(4),
                updated_at,
            },
        }
    }

    #[test]
    fn continue_prefers_recent_unfinished_reading_and_next_unread_keeps_chapter_order() {
        let books = vec![
            book("1", ReadingStatus::Completed, 30),
            book("2", ReadingStatus::Reading, 10),
            book("3", ReadingStatus::Unread, 0),
            book("4", ReadingStatus::Reading, 20),
            book("5", ReadingStatus::Unread, 0),
        ];
        assert_eq!(reading_target(&books).unwrap().id, "4");
        assert_eq!(next_unread(&books).unwrap().id, "3");
        assert_eq!(reading_target(&books[..3]).unwrap().id, "2");
        assert!(reading_target(&books[..1]).is_none());
        assert!(next_unread(&[]).is_none());
        let stats = SeriesProgress::from_books(&books);
        assert_eq!(
            stats,
            SeriesProgress {
                total: 5,
                completed: 1,
                reading: 2
            }
        );
    }

    #[test]
    fn unavailable_local_books_are_skipped_but_indexed_cloud_books_can_be_next() {
        let mut books = vec![
            book("missing", ReadingStatus::Unread, 0),
            book("drive-cloud", ReadingStatus::Unread, 0),
        ];
        books.iter_mut().for_each(|b| b.downloaded = false);
        assert_eq!(next_unread(&books).unwrap().id, "drive-cloud");
        assert!(reading_target(&books[..1]).is_none());
    }

    #[test]
    fn progress_handles_unknown_counts_and_never_infers_completion_in_the_ui() {
        let mut reading = ReadingProgress {
            status: ReadingStatus::Reading,
            last_page: Some(0),
            updated_at: 1,
        };
        assert_eq!(reading.label(Some(10)), "Reading · page 1 of 10");
        assert_eq!(reading.percent(Some(10)), Some(10));
        assert_eq!(reading.percent(None), None);
        assert_eq!(reading.percent(Some(0)), None);
        reading.last_page = Some(999);
        assert_eq!(reading.percent(Some(10)), Some(99));
        reading.status = ReadingStatus::Completed;
        assert_eq!(reading.percent(None), Some(100));
        assert_eq!(ReadingProgress::default().label(None), "Unread");
    }
}
