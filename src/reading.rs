use crate::types::{ComicBook, ReadingProgress, ReadingStatus};

impl ReadingProgress {
    pub fn label(&self, page_count: Option<u32>) -> String {
        match self.status {
            ReadingStatus::Unread => "Unread".into(),
            ReadingStatus::Completed => "Completed".into(),
            ReadingStatus::Reading => match (self.last_page, page_count.filter(|n| *n > 0)) {
                (Some(page), Some(total)) => format!("Reading · page {} of {total}", page.saturating_add(1).min(total)),
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
            completed: books.iter().filter(|b| b.reading.status == ReadingStatus::Completed).count(),
            reading: books.iter().filter(|b| b.reading.status == ReadingStatus::Reading).count(),
        }
    }

    pub fn label(self) -> String {
        let mut label = format!("{} of {} completed", self.completed, self.total);
        if self.reading > 0 { label.push_str(&format!(" · {} in progress", self.reading)); }
        label
    }
}

fn available(book: &&ComicBook) -> bool { book.downloaded || book.id.starts_with("drive-") }

/// Call with the visible, ordered collection so actions respect filters and
/// never leave the selected series. Continue unfinished reading before starting
/// another book; explicit "next unread" skips only completed/started books.
pub fn next_unread(books: &[ComicBook]) -> Option<&ComicBook> {
    books.iter().filter(available).find(|b| b.reading.status == ReadingStatus::Unread)
}

pub fn reading_target(books: &[ComicBook]) -> Option<&ComicBook> {
    books.iter().filter(available).filter(|b| b.reading.status == ReadingStatus::Reading)
        .max_by_key(|b| b.reading.updated_at).or_else(|| next_unread(books))
}
