use std::path::{Path, PathBuf};

#[path = "src/drive/client.rs"]
mod google_oauth_client;

fn main() {
    bundle_google_oauth_client();
    copy_pdfium_next_to_binary();
    tauri_build::build()
}

/// Configure the publisher's client once per build. User refresh/access tokens
/// are deliberately excluded from the generated resource.
fn bundle_google_oauth_client() {
    println!("cargo:rerun-if-env-changed=COSMIC_GOOGLE_OAUTH_CLIENT_JSON");
    println!("cargo:rerun-if-changed=src/drive/client.rs");
    let explicit = std::env::var_os("COSMIC_GOOGLE_OAUTH_CLIENT_JSON");
    let path = explicit
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("google-drive-client.json"));
    println!("cargo:rerun-if-changed={}", path.display());
    let client = match std::fs::read(&path) {
        Ok(bytes) => Some(
            google_oauth_client::OAuthClient::from_google_json(&bytes)
                .unwrap_or_else(|error| panic!("Google Drive build configuration: {error}")),
        ),
        Err(error) if explicit.is_none() && error.kind() == std::io::ErrorKind::NotFound => {
            println!("cargo:warning=Google Drive is disabled in this build. Configure the publisher OAuth client as described in README.md.");
            None
        }
        Err(error) => panic!("Could not read the Google Drive build configuration: {error}"),
    };
    let output = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo provides OUT_DIR"))
        .join("google-drive-client.json");
    std::fs::write(
        output,
        serde_json::to_vec(&client).expect("OAuth client is serializable"),
    )
    .expect("Could not write the bundled Google OAuth client");
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
