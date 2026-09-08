use std::path::Path;

pub mod cb7;
pub mod cbr;
pub mod cbz;
pub mod pdf;

pub use cbz::CbzArchive;
pub use pdf::PdfArchive;

pub trait ComicArchive: Send {
    fn page_count(&self) -> anyhow::Result<u32>;
    fn get_page_bytes(&mut self, index: u32) -> anyhow::Result<Vec<u8>>;

    fn get_cover_bytes(&mut self) -> anyhow::Result<Vec<u8>> {
        self.get_page_bytes(0)
    }
}

/// Pages already decompressed into memory.
///
/// ponytail: 7z and RAR both expose forward-only cursors, so random page
/// access means holding the whole archive. Fine for typical comics; if large
/// archives become a problem, extract to a temp dir and read pages back.
pub struct EagerPages(Vec<Vec<u8>>);

impl EagerPages {
    /// Order pages naturally by entry name, then keep only the bytes.
    pub fn new(mut named: Vec<(String, Vec<u8>)>) -> Self {
        named.sort_by(|(a, _), (b, _)| natord::compare(a, b));
        EagerPages(named.into_iter().map(|(_, bytes)| bytes).collect())
    }
}

impl ComicArchive for EagerPages {
    fn page_count(&self) -> anyhow::Result<u32> {
        Ok(self.0.len() as u32)
    }

    fn get_page_bytes(&mut self, index: u32) -> anyhow::Result<Vec<u8>> {
        self.0
            .get(index as usize)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Page {} out of range", index))
    }
}

pub fn open_archive(path: &Path) -> anyhow::Result<Box<dyn ComicArchive + Send>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "cbz" | "zip" => Ok(Box::new(CbzArchive::open(path)?)),
        "cbr" | "rar" => Ok(Box::new(cbr::open(path)?)),
        "cb7" | "7z" => Ok(Box::new(cb7::open(path)?)),
        "pdf" => Ok(Box::new(PdfArchive::open(path)?)),
        other => anyhow::bail!("Unsupported format: .{}", other),
    }
}

pub fn is_image_file(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".png")
        || lower.ends_with(".webp")
        || lower.ends_with(".gif")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(name: &str, byte: u8) -> (String, Vec<u8>) {
        (name.to_string(), vec![byte])
    }

    #[test]
    fn eager_pages_order_naturally() {
        // The order CBR and CB7 rely on: page 2 before page 10.
        let mut pages = EagerPages::new(vec![
            page("p10.jpg", 10),
            page("p2.jpg", 2),
            page("p1.jpg", 1),
        ]);
        assert_eq!(pages.page_count().unwrap(), 3);
        assert_eq!(pages.get_page_bytes(0).unwrap(), vec![1]);
        assert_eq!(pages.get_page_bytes(1).unwrap(), vec![2]);
        assert_eq!(pages.get_page_bytes(2).unwrap(), vec![10]);
    }

    #[test]
    fn eager_pages_reject_out_of_range() {
        let mut pages = EagerPages::new(vec![page("only.jpg", 1)]);
        assert!(pages.get_page_bytes(1).is_err());
        assert_eq!(pages.get_cover_bytes().unwrap(), vec![1]);
    }

    #[test]
    fn image_files_recognised_by_extension() {
        for name in ["a.jpg", "A.JPEG", "b.png", "c.webp", "d.gif"] {
            assert!(is_image_file(name), "{name}");
        }
        for name in ["ComicInfo.xml", "notes.txt", "cover.jpg.bak", ""] {
            assert!(!is_image_file(name), "{name}");
        }
    }

    #[test]
    fn cbr_is_routed_to_the_rar_reader() {
        // Not "Unsupported format" - proves .cbr reaches the unrar path.
        let err = open_archive(Path::new("/nonexistent/x.cbr")).err().unwrap().to_string();
        assert!(!err.contains("Unsupported format"), "got: {err}");

        let err = open_archive(Path::new("/nonexistent/x.txt")).err().unwrap().to_string();
        assert!(err.contains("Unsupported format"), "got: {err}");
    }
}
