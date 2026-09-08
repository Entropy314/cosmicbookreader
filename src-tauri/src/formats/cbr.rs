use std::path::Path;

use unrar_ng::Archive;

use super::{is_image_file, EagerPages};

/// Read every image entry out of a RAR/CBR archive.
///
/// unrar exposes a forward-only cursor: each header is either read or skipped,
/// and doing so hands back the archive positioned at the next one.
pub fn open(path: &Path) -> anyhow::Result<EagerPages> {
    let mut archive = Archive::new(path)
        .open_for_processing()
        .map_err(|e| anyhow::anyhow!("Failed to open RAR archive: {}", e))?;

    let mut named_pages: Vec<(String, Vec<u8>)> = Vec::new();

    while let Some(header) = archive
        .read_header()
        .map_err(|e| anyhow::anyhow!("Failed to read RAR entry: {}", e))?
    {
        let entry = header.entry();
        let name = entry.filename.to_string_lossy().to_string();

        archive = if entry.is_file() && is_image_file(&name) {
            let (bytes, rest) = header
                .read()
                .map_err(|e| anyhow::anyhow!("Failed to read '{}': {}", name, e))?;
            named_pages.push((name, bytes));
            rest
        } else {
            header
                .skip()
                .map_err(|e| anyhow::anyhow!("Failed to skip '{}': {}", name, e))?
        };
    }

    Ok(EagerPages::new(named_pages))
}
