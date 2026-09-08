use std::io::Read;
use std::path::Path;

use sevenz_rust::{Password, SevenZReader};

use super::{is_image_file, EagerPages};

/// Read every image entry out of a 7z/CB7 archive.
pub fn open(path: &Path) -> anyhow::Result<EagerPages> {
    let file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let mut reader = SevenZReader::new(file, len, Password::empty())
        .map_err(|e| anyhow::anyhow!("Failed to open 7z archive: {}", e))?;

    let mut named_pages: Vec<(String, Vec<u8>)> = Vec::new();

    reader
        .for_each_entries(&mut |entry: &sevenz_rust::SevenZArchiveEntry, reader: &mut dyn Read| {
            let name = entry.name().to_string();
            if !entry.is_directory() && is_image_file(&name) {
                let mut bytes = Vec::new();
                reader.read_to_end(&mut bytes)?;
                named_pages.push((name, bytes));
            }
            Ok(true)
        })
        .map_err(|e| anyhow::anyhow!("Failed to read 7z entries: {}", e))?;

    Ok(EagerPages::new(named_pages))
}
