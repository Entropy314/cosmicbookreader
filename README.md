# Cosmic Book Reader

A desktop comic book reader. Tauri 2 backend, Leptos 0.7 (client-side) frontend.

## Formats

| Format | Status |
| --- | --- |
| `.cbz` / `.zip` | Supported |
| `.cb7` / `.7z` | Supported |
| `.pdf` | Supported (PDFium ships with the app) |
| `.cbr` / `.rar` | Supported |

## Running

```sh
cargo tauri dev      # dev build with hot reload
cargo tauri build    # release bundle
```

Requires the `wasm32-unknown-unknown` target and `trunk`:

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk tauri-cli
```

## PDF support

PDF rendering uses PDFium, bundled at `src-tauri/lib/libpdfium.dylib` (from
[pdfium-binaries](https://github.com/bblanchon/pdfium-binaries/releases)) and
shipped into the app bundle's resources. At runtime the loader looks beside the
executable, in `lib/`, and in the macOS `Resources` directory before falling
back to a system install.

The checked-in binary is macOS arm64. Building for another platform means
dropping that platform's `libpdfium` into `src-tauri/lib/`. If it is missing,
PDFs fail to open with an error naming the expected file and directory, and
every other format keeps working.

RAR support uses the UnRAR source, which permits decompression but forbids
using it to recreate the RAR compression algorithm.

## Usage

**Library.** "Open Folder" scans a directory tree for comics; "Add Files"
picks individual ones. The chosen folder is remembered across launches. Toggle
between a flat grid and a series view, and search by title or series.

Right-click a comic or series for remove/delete actions. In a series, click to
multi-select, `Ctrl+A` selects all visible, `Esc` clears.

**Reader.**

| Key | Action |
| --- | --- |
| `→` `↓` `Space` `PageDown` | Next page |
| `←` `↑` `PageUp` | Previous page |
| `]` / `[` | Next / previous chapter |
| `Home` / `End` | First / last page |
| `+` / `-` | Zoom in / out |
| `F` | Cycle fit mode (page, width, height, free) |
| `F11` | Fullscreen |
| `T` | Toggle toolbar |
| `Esc` | Back to library |

Clicking the left, middle, and right thirds of the page turns back, toggles
the toolbar, and turns forward.

Turning past the end of a chapter rolls into the next one in the same series,
and turning back from the first page rolls into the previous one. Each chapter
reopens wherever you left it.

The toolbar's ☛/☚ button switches the series between left-to-right and
right-to-left (manga) order, which swaps what the ← → keys and the left and
right click zones do. The choice is remembered per series. Fit mode and zoom
carry across chapters.

## Layout

```text
src/          Leptos frontend (wasm)
src-tauri/    Tauri backend
  formats/    Archive readers, one per format, behind the ComicArchive trait
  commands/   IPC command handlers
  cache/      SQLite metadata + on-disk JPEG thumbnails
```

Covers are cached as 200px JPEGs in the app cache directory, keyed by file
mtime, alongside a SQLite database holding library metadata.
