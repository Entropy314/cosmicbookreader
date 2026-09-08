use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

use pdfium_render::prelude::*;

use super::ComicArchive;

pub struct PdfArchive {
    path: PathBuf,
    count: u32,
}

impl PdfArchive {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let _guard = pdf_lock();
        let doc = pdfium()?
            .load_pdf_from_file(path, None)
            .map_err(|e| anyhow::anyhow!("Failed to open PDF: {}", e))?;
        let count = doc.pages().len() as u32;
        Ok(PdfArchive { path: path.to_path_buf(), count })
    }
}

/// Width to render a page for reading. Covers use far less - see below.
const READ_WIDTH: Pixels = 1400;

/// Covers are downscaled to a 200px thumbnail, so rendering a full-size page
/// for one wastes roughly 50x the pixels. 400 leaves headroom for a 2x display.
const COVER_WIDTH: Pixels = 400;

impl PdfArchive {
    fn render(&self, index: u32, target_width: Pixels) -> anyhow::Result<Vec<u8>> {
        let _guard = pdf_lock();

        // ponytail: the document is reparsed per page. PdfDocument holds a raw
        // FPDF_DOCUMENT and is not Send, so it cannot be stored in ActiveReader
        // (Box<dyn ComicArchive + Send>). Binding the library is the expensive
        // half and is cached below; caching the document too would mean giving
        // the reader its own PDF thread to own the handle.
        let doc = pdfium()?
            .load_pdf_from_file(&self.path, None)
            .map_err(|e| anyhow::anyhow!("Failed to open PDF: {}", e))?;

        let config = PdfRenderConfig::new().set_target_width(target_width);
        let page = doc
            .pages()
            .get(index as u16)
            .map_err(|e| anyhow::anyhow!("Failed to get page {}: {}", index, e))?;
        let bitmap = page
            .render_with_config(&config)
            .map_err(|e| anyhow::anyhow!("Render failed: {}", e))?;
        let img = bitmap.as_image();

        let mut jpeg_bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut jpeg_bytes), image::ImageFormat::Jpeg)
            .map_err(|e| anyhow::anyhow!("JPEG encode failed: {}", e))?;
        Ok(jpeg_bytes)
    }
}

impl ComicArchive for PdfArchive {
    fn page_count(&self) -> anyhow::Result<u32> {
        Ok(self.count)
    }

    fn get_page_bytes(&mut self, index: u32) -> anyhow::Result<Vec<u8>> {
        self.render(index, READ_WIDTH)
    }

    fn get_cover_bytes(&mut self) -> anyhow::Result<Vec<u8>> {
        self.render(0, COVER_WIDTH)
    }
}

/// Serialises every call into PDFium.
///
/// ponytail: one global lock. PDFium is not safe for concurrent use across
/// documents, and this crate's `thread_safe` bindings delegate individual calls
/// without locking - racing them segfaults. Reading is sequential anyway; only
/// cover generation contends, and it simply queues. Revisit with per-document
/// locks or a dedicated PDF thread only if PDF throughput starts to matter.
static PDF_LOCK: Mutex<()> = Mutex::new(());

/// A panic while rendering must not disable PDF for the rest of the session.
fn pdf_lock() -> MutexGuard<'static, ()> {
    PDF_LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The one and only `Pdfium` for this process.
///
/// This must never be dropped: `Pdfium::drop` calls `FPDF_DestroyLibrary`,
/// which tears down PDFium's *global* state, so a second instance being
/// created and discarded breaks the instance everyone else is holding. Building
/// it inside `get_or_init` guarantees exactly one is ever constructed.
static PDFIUM: OnceLock<Option<Pdfium>> = OnceLock::new();

/// Bind to the system PDFium once per process.
///
/// Binding re-resolves the library's whole symbol table, which is far too slow
/// to repeat on every page turn. `Pdfium` is Sync and the `thread_safe` feature
/// serialises calls internally, so one shared instance is safe.
fn pdfium() -> anyhow::Result<&'static Pdfium> {
    PDFIUM
        .get_or_init(|| {
            bind_bundled()
                .or_else(|| Pdfium::bind_to_system_library().ok())
                .map(Pdfium::new)
        })
        .as_ref()
        .ok_or_else(|| {
            anyhow::anyhow!(
                "PDFium library not found. Download {} from \
                 https://github.com/bblanchon/pdfium-binaries/releases and put it \
                 next to the application binary ({}).",
                Pdfium::pdfium_platform_library_name().to_string_lossy(),
                std::env::current_exe()
                    .ok()
                    .and_then(|e| e.parent().map(|p| p.display().to_string()))
                    .unwrap_or_else(|| "the executable's directory".into()),
            )
        })
}

/// Look for libpdfium shipped with the app before falling back to the system.
///
/// Covers `cargo tauri dev` (next to the debug binary) and a bundled release
/// (macOS puts resources in `../Resources`, other platforms alongside).
fn bind_bundled() -> Option<Box<dyn PdfiumLibraryBindings>> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    // `..` also covers test binaries, which run from target/debug/deps.
    [dir.to_path_buf(), dir.join("lib"), dir.join("../Resources"), dir.join("..")]
        .into_iter()
        .find_map(|candidate| {
            Pdfium::bind_to_library(Pdfium::pdfium_platform_library_name_at_path(&candidate)).ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::open_archive;

    /// A minimal one-page PDF, built with correct xref offsets.
    fn minimal_pdf() -> Vec<u8> {
        let objects = [
            "<</Type/Catalog/Pages 2 0 R>>",
            "<</Type/Pages/Kids[3 0 R]/Count 1>>",
            "<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 200]/Resources<<>>>>",
        ];

        let mut out = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (i, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.push_str(&format!("{} 0 obj\n{}\nendobj\n", i + 1, body));
        }

        let xref_at = out.len();
        out.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1));
        for offset in &offsets {
            out.push_str(&format!("{offset:010} 00000 n \n"));
        }
        out.push_str(&format!(
            "trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{xref_at}\n%%EOF\n",
            objects.len() + 1
        ));
        out.into_bytes()
    }

    #[test]
    fn concurrent_pdf_access_is_serialised() {
        // Regression: PDFium is not safe for concurrent use and this crate's
        // thread_safe bindings do not lock individual calls, so generating
        // covers for several PDFs at once segfaulted the whole process.
        //
        // Caveat: these synthetic pages are too simple to reproduce that
        // segfault - it needed real pages with large embedded images. Against
        // the unsynchronised code this test hangs rather than failing cleanly,
        // so treat a timeout here as a failure, not flakiness.
        let dir = std::env::temp_dir().join("kbr-pdf-race");
        std::fs::create_dir_all(&dir).unwrap();

        let paths: Vec<PathBuf> = (0..8)
            .map(|i| {
                let p = dir.join(format!("race-{i}.pdf"));
                std::fs::write(&p, minimal_pdf()).unwrap();
                p
            })
            .collect();

        let handles: Vec<_> = paths
            .into_iter()
            .map(|p| {
                std::thread::spawn(move || {
                    let mut archive = open_archive(&p).expect("open pdf");
                    assert_eq!(archive.page_count().expect("count"), 1);
                    archive.get_page_bytes(0).expect("render page");
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("thread died - PDFium was raced");
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
