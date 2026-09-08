use std::path::{Path, PathBuf};

fn main() {
    copy_pdfium_next_to_binary();
    tauri_build::build()
}

/// Put the bundled PDFium beside the built executable.
///
/// `cargo tauri dev` and the test binaries load it from there, so copying it
/// here means PDF works from a fresh clone with no manual setup. Release
/// bundles get it through `bundle.resources` in tauri.conf.json instead.
fn copy_pdfium_next_to_binary() {
    let name = if cfg!(target_os = "windows") {
        "pdfium.dll"
    } else if cfg!(target_os = "macos") {
        "libpdfium.dylib"
    } else {
        "libpdfium.so"
    };

    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("lib").join(name);
    println!("cargo:rerun-if-changed=lib/{name}");
    if !src.exists() {
        // Not fatal: PDF reports a clear error at runtime, other formats work.
        println!("cargo:warning=PDFium not found at lib/{name}; PDF support will be unavailable");
        return;
    }

    // OUT_DIR is <target>/<profile>/build/<pkg>-<hash>/out
    let Some(target_dir) = std::env::var_os("OUT_DIR")
        .map(PathBuf::from)
        .and_then(|out| out.ancestors().nth(3).map(Path::to_path_buf))
    else {
        return;
    };

    let dest = target_dir.join(name);
    let stale = std::fs::metadata(&dest)
        .and_then(|d| d.modified())
        .ok()
        .zip(std::fs::metadata(&src).and_then(|s| s.modified()).ok())
        .map(|(dest_time, src_time)| dest_time < src_time)
        .unwrap_or(true);

    if stale {
        let _ = std::fs::copy(&src, &dest);
    }
}
