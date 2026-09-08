use std::io::Cursor;

use image::GenericImageView;

use crate::formats::ComicArchive;

/// Extract and resize cover image to a JPEG thumbnail (max 200px wide).
pub fn extract_thumbnail(archive: &mut dyn ComicArchive) -> anyhow::Result<Vec<u8>> {
    let cover_bytes = archive.get_cover_bytes()?;
    resize_to_thumbnail(&cover_bytes)
}

/// Decode image bytes and resize to max 200px wide, returning JPEG bytes.
pub fn resize_to_thumbnail(bytes: &[u8]) -> anyhow::Result<Vec<u8>> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| anyhow::anyhow!("Failed to decode image: {}", e))?;

    let (w, h) = img.dimensions();
    let thumb = if w > 200 {
        // .max(1): an extreme aspect ratio must not round the height to zero.
        let new_h = ((h as f64 * 200.0 / w as f64).round() as u32).max(1);
        img.resize(200, new_h, image::imageops::FilterType::Triangle)
    } else {
        img
    };

    let (tw, th) = thumb.dimensions();
    let mut jpeg_bytes = Vec::new();
    {
        let mut cursor = Cursor::new(&mut jpeg_bytes);
        let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut cursor, 72);
        encoder
            .encode(
                thumb.to_rgb8().as_raw(),
                tw,
                th,
                image::ColorType::Rgb8.into(),
            )
            .map_err(|e| anyhow::anyhow!("JPEG encode error: {}", e))?;
    }

    Ok(jpeg_bytes)
}
