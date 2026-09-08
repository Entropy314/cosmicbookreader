use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

use super::{is_image_file, ComicArchive};

pub struct CbzArchive {
    archive: ZipArchive<File>,
    image_indices: Vec<usize>,
}

impl CbzArchive {
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        let file = File::open(path)?;
        let mut archive = ZipArchive::new(file)?;

        let mut named_indices: Vec<(String, usize)> = Vec::new();
        for i in 0..archive.len() {
            if let Ok(entry) = archive.by_index(i) {
                let name = entry.name().to_string();
                if is_image_file(&name) && !entry.is_dir() {
                    named_indices.push((name, i));
                }
            }
        }

        named_indices.sort_by(|(a, _), (b, _)| natord::compare(a, b));
        let image_indices = named_indices.into_iter().map(|(_, i)| i).collect();

        Ok(CbzArchive { archive, image_indices })
    }
}

impl ComicArchive for CbzArchive {
    fn page_count(&self) -> anyhow::Result<u32> {
        Ok(self.image_indices.len() as u32)
    }

    fn get_page_bytes(&mut self, index: u32) -> anyhow::Result<Vec<u8>> {
        let zip_index = self
            .image_indices
            .get(index as usize)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Page {} out of range", index))?;

        let mut entry = self.archive.by_index(zip_index)?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::extract_thumbnail;
    use std::io::Write;

    /// A PNG `w` pixels wide with a page-like aspect ratio, so pages can be
    /// told apart after decoding.
    fn png(w: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::new(w, w * 3 / 2))
            .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    /// Build a real .cbz whose page `n` is an image `n` pixels wide.
    fn write_cbz(tag: &str, entries: &[(&str, u32)]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("kbr-{tag}.cbz"));
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, w) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&png(*w)).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    fn width_of(bytes: &[u8]) -> u32 {
        use image::GenericImageView;
        image::load_from_memory(bytes).unwrap().dimensions().0
    }

    #[test]
    fn pages_read_in_natural_order_ignoring_non_images() {
        let path = write_cbz(
            "order",
            &[("p10.png", 10), ("p2.png", 2), ("ComicInfo.xml", 4), ("p1.png", 1)],
        );
        let mut archive = CbzArchive::open(&path).unwrap();

        // ComicInfo.xml is not a page.
        assert_eq!(archive.page_count().unwrap(), 3);
        // p1, p2, p10 - not p1, p10, p2.
        assert_eq!(width_of(&archive.get_page_bytes(0).unwrap()), 1);
        assert_eq!(width_of(&archive.get_page_bytes(1).unwrap()), 2);
        assert_eq!(width_of(&archive.get_page_bytes(2).unwrap()), 10);
        assert!(archive.get_page_bytes(3).is_err());

        std::fs::remove_file(path).ok();
    }

    #[test]
    fn thumbnail_downscales_the_cover() {
        let path = write_cbz("thumb", &[("cover.png", 800)]);
        let mut archive = CbzArchive::open(&path).unwrap();
        let thumb = extract_thumbnail(&mut archive).unwrap();
        assert_eq!(width_of(&thumb), 200, "cover should be resized to 200px");
        std::fs::remove_file(path).ok();
    }
}
