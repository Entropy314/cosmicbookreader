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

Use `cargo tauri dev`, not `cargo run tauri`: the latter runs the browser
frontend as a native executable instead of launching Tauri.

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

macOS arm64 (`libpdfium.dylib`) and Windows x64 (`pdfium.dll`) binaries are
checked in. Building for another platform means dropping that platform's
`libpdfium` into `src-tauri/lib/`. If it is missing, PDFs fail to open with
an error naming the expected file and directory, and every other format
keeps working.

RAR support uses the UnRAR source, which permits decompression but forbids
using it to recreate the RAR compression algorithm.

## Usage

**Library.** **Add books → Open a folder** scans a directory tree for comics;
**Add books → Choose files** adds individual books. The chosen folder is remembered across launches. Toggle
between **By title** (the default) and **All books**, and search by title or chapter.
Each title has one collection card containing its chapters, volumes, and issues
in reading order. Grouping handles chapter prefixes, uploader tags, and capitalization
variants. Drive folder names identify files with names such as `Chapter 001.cbz`;
run **Sync now** once after upgrading to add that folder context to an existing index.
Grouping and browsing an index do not download book files.

Filter by **Everything**, **Available offline**, or **Not downloaded**. Titles
open into a chapter list with **Read** or **Download & read** actions. Library
search is preserved when returning from a title. **Continue reading** resumes
an unfinished book at its saved page. **Read next unread** opens the first
unread book in chapter order, within the current title and availability filters.

Books show **Unread**, **Reading**, or **Completed**, with page progress for
opened books. Opening the first page starts reading; displaying the last page
marks the book completed. Completion stays recorded when revisiting earlier
pages. Title cards and title pages show the number of completed books.
Use **⋯ → Mark completed** or **Mark unread · reset progress** on a book, title,
or selection. Marking unread resets its saved page to the beginning. These
actions work on indexed books without downloading them. Progress survives
restarts, rescans, and Drive syncs; existing saved positions are migrated.

Use checkboxes to select books and **Selection actions** for bulk changes.
The **⋯** button (or right-click) opens book/title actions. Deleting local files
asks for confirmation; Drive originals stay unchanged. `Ctrl+A` / `Cmd+A`
select all visible books outside text fields, and `Esc` clears the selection.

The reading-progress browser regression check uses a fake Tauri bridge and
requires no Google account:

```sh
NO_COLOR=true trunk build --dist /tmp/reader-test-dist
python3 tests/reading_progress_ui.py --dist /tmp/reader-test-dist --browser /path/to/chrome-headless-shell
```

## Google Drive collection

Click **Google Drive** in the library toolbar to connect one collection folder.
By default, **Index only** adds supported comics from that folder and its
subfolders to your library using metadata only. A comic downloads when you open
it and is then cached for offline reading. The app checks for catalog changes
on launch and every five minutes while running. **Sync now**, **Cancel**, an
automatic sync toggle, and the sync mode selector are in the same panel.

### For users

1. Open **Google Drive** in the library toolbar.
2. Paste the URL or ID of your own collection folder.
3. Keep **As I read · Index only (recommended)**, or choose **Download the whole
   collection** if you want every book stored locally.
4. Click **Sign in with Google**, choose your Google account in the browser,
   and allow read access. The initial sync starts automatically.

Users do not need a Google Cloud project, API key, or client JSON. Each
installation stores its user's authorization in the operating system credential
store. The app supports one connected Google account and collection folder per
OS user at a time. Open **Sync settings & collection folder**, then use **Connect this folder** to sign into another account.

### Publisher setup (once for the distributed app)

The app publisher supplies a single Google OAuth Desktop client when building
the app. Each user signs into that client with their own account; the build
contains no publisher or user access/refresh token.

1. Create a project in [Google Cloud Console](https://console.cloud.google.com/)
   and enable the [Google Drive API](https://console.cloud.google.com/apis/library/drive.googleapis.com).
2. Configure [Google Auth Platform](https://console.cloud.google.com/auth/overview)
   with the app's branding and **External** audience to support users outside
   your organization. Declare `https://www.googleapis.com/auth/drive.readonly`
   under Data Access. Add test users while developing.
3. Create an OAuth client of type **Desktop app**, download its JSON, and save
   it as `src-tauri/google-drive-client.json` (ignored by Git). A Web application
   client or service account is not suitable for this desktop sign-in flow.
4. Run `cargo tauri dev` to test or `cargo tauri build` to distribute the app.
   The build validates the client and embeds only its client ID and Desktop
   client secret into the native backend. Users need only the installed app.

For CI or a JSON file stored elsewhere, provide an absolute path:

```sh
COSMIC_GOOGLE_OAUTH_CLIENT_JSON=/absolute/path/desktop-client.json cargo tauri build
```

The JSON is a **build-time** input. Installed apps do not need that file or the
environment variable. Cargo watches changes to the file and variable. An
explicitly configured missing/invalid file fails the build; an unconfigured
local build still supports local comics and displays Drive as unavailable.

Before public release, publish the OAuth app to Production and complete Google's
required verification for the restricted Drive read-only scope. Configure your
app homepage, privacy policy, support/contact details, and verified domains as
required. Testing mode is limited to listed test users; it is not the public
user experience. Google's [restricted-scope verification guide](https://developers.google.com/identity/protocols/oauth2/production-readiness/restricted-scope-verification)
describes these requirements and when a security assessment applies. Google
Cloud setup and approval cannot be completed by a local source-code change.

The app uses PKCE and a loopback callback in the system browser, per Google's
[installed-app OAuth guide](https://developers.google.com/identity/protocols/oauth2/native-app).
Desktop clients are public clients: an embedded Desktop client secret cannot
be treated as confidential. Each user's refresh token **is** confidential and
is stored with the client identity in macOS Keychain, Windows Credential Manager,
or Linux Secret Service, never in the frontend or library database. Drive
requests go directly from the user's computer to Google. Linux builds need a
running Secret Service and D-Bus development libraries (`libdbus-1-dev` on
Debian/Ubuntu).

Read-only scope permits reading all Drive files; the app traverses only the
selected folder. `drive.file` does not grant recursive access to an arbitrary
existing collection. See Google's [scope guide](https://developers.google.com/workspace/drive/api/guides/api-specific-auth).
In Testing mode, refresh tokens for this scope normally expire after seven days;
see [token expiration](https://developers.google.com/identity/protocols/oauth2#expiration).
Upgrading from the older manual-client setup, or changing the publisher client,
requires reconnecting. Existing downloads and reading progress remain available.

### Sync behavior

- This is **one-way, Drive → computer**. The app never uploads, edits, or deletes
  files in Google Drive. Reading progress and manga direction stay on this device.
- **Index only** syncs names, file IDs, sizes, versions, and library groupings.
  It does not fetch comic archives or PDFs. Indexed books are marked **CLOUD**.
  Opening one downloads that full file, shows a cancellable loading screen,
  and then opens it in the reader. Cached books are marked **OFFLINE** and can
  be reopened without a network connection. Covers are generated only from
  downloaded files; browsing the library never downloads archives for covers.
- **Download all for offline reading** downloads the full supported collection
  on the next sync, so allow enough disk space. Unchanged downloaded versions
  are reused. Both modes preserve reading progress across renames and updates.
- The default for new and previously configured installations is **Index only**
  unless a sync mode was explicitly saved. Changing to index-only keeps all
  existing downloads. Cancel a running operation before changing modes, then
  use **Sync now** to apply the mode immediately.
- A successful sync removes library entries for files moved out of the selected
  folder or deleted from Drive. Failed, cancelled, or incomplete syncs keep the
  previous library. Downloads finish in temporary files before being published.
- **Remove** or **Delete file** acts locally. An entry still in the Drive folder
  returns on the next sync. In index-only mode, its file is downloaded again
  only when opened; offline mode downloads it on the next sync.
- **Disconnect** forgets the credentials on this computer and keeps downloaded
  books and progress. Reconnecting only requires Google sign-in again.
  Indexed books without a local download require reconnection before reading.
  You can also revoke the grant in [Google Account connections](https://myaccount.google.com/connections).
- Changing folders replaces the Drive portion of the library after a successful
  sync. Local imports are preserved. Old downloaded versions are retained on
  disk so an open reader is never overwritten; automatic disk cleanup is not
  implemented.
- Shared folders/drives work when your account can list and download their
  contents. Drive shortcuts, Google Docs, and folders of loose page images are
  skipped; use supported archives or PDFs.

Downloads and non-secret settings live under the app data directory in
`google-drive/` (on macOS, `~/Library/Application Support/com.entropy.cosmicbookreader/google-drive/`).

## Reader controls

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

The toolbar's **Left to right / Right to left** button switches the series between left-to-right and
right-to-left (manga) order, which swaps what the ← → keys and the left and
right click zones do. The choice is remembered per series. Fit mode and zoom
carry across chapters. Choose a fit mode from the dropdown, or enter a page
number to jump directly to it.

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
