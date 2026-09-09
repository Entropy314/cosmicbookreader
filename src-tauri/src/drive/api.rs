use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

use reqwest::{Response, StatusCode, Url};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;

use super::auth::Session;
use crate::types::ComicFormat;

const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
const FIELDS: &str = "id,name,mimeType,version,size,trashed";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriveFile {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    #[serde(default)]
    pub version: String,
    pub size: Option<String>,
    #[serde(default)]
    pub trashed: bool,
    /// Ancestor names collected during listing, from the selected root down.
    #[serde(skip)]
    pub folder_path: Vec<String>,
}

impl DriveFile {
    pub fn extension(&self) -> String {
        Path::new(&self.name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase()
    }

    pub fn is_comic(&self) -> bool {
        !self.mime_type.starts_with("application/vnd.google-apps.")
            && ComicFormat::from_extension(&self.extension()) != ComicFormat::Unknown
    }

    pub fn file_size(&self) -> Result<u64, String> {
        self.size
            .as_deref()
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| format!("Google did not return a file size for {}.", self.name))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileList {
    #[serde(default)]
    files: Vec<DriveFile>,
    next_page_token: Option<String>,
    #[serde(default)]
    incomplete_search: bool,
}

pub fn folder_id(input: &str) -> Result<String, String> {
    let input = input.trim();
    let id = if input.starts_with("https://") {
        let url =
            Url::parse(input).map_err(|_| "Enter a Google Drive folder link or ID.".to_string())?;
        if url.host_str() != Some("drive.google.com") {
            return Err("Use a folder link from drive.google.com.".into());
        }
        let segments: Vec<_> = url.path_segments().into_iter().flatten().collect();
        let index = segments
            .iter()
            .position(|s| *s == "folders")
            .ok_or("Use a Google Drive folder link, not a file link.".to_string())?;
        segments
            .get(index + 1)
            .ok_or("The folder link is missing its ID.".to_string())?
            .to_string()
    } else {
        input.to_string()
    };
    if id.is_empty()
        || id.len() > 256
        || !id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
    {
        return Err("Enter a valid Google Drive folder link or folder ID.".into());
    }
    Ok(id)
}

fn file_url(session: &Session, id: &str) -> Result<Url, String> {
    let mut url = session.files_url.clone();
    url.path_segments_mut()
        .map_err(|_| "Invalid Drive API URL.".to_string())?
        .push(id);
    Ok(url)
}

// GET requests are idempotent. Retry bounded transient failures without
// exposing response bodies (which may contain account data) in errors.
async fn get(
    session: &mut Session,
    url: Url,
    query: &[(&str, &str)],
    download: bool,
) -> Result<Response, String> {
    for attempt in 0..4 {
        let token = session.token().await?;
        let response = session
            .client
            .get(url.clone())
            .query(query)
            .bearer_auth(token)
            .timeout(Duration::from_secs(if download { 3600 } else { 60 }))
            .send()
            .await;
        match response {
            Ok(response) if response.status().is_success() => return Ok(response),
            Ok(response) if (response.status() == StatusCode::TOO_MANY_REQUESTS || response.status().is_server_error()) && attempt < 3 => {},
            Ok(response) => return Err(match response.status() {
                StatusCode::UNAUTHORIZED => "Google authorization expired. Reconnect Drive.",
                StatusCode::FORBIDDEN => "Google denied access. Check that the Drive API is enabled and your account can read and download this folder.",
                StatusCode::NOT_FOUND => "This Drive folder or file is no longer available to your account.",
                _ => "Google Drive could not complete the request. Try syncing again.",
            }.into()),
            Err(_) if attempt < 3 => {},
            Err(_) => return Err("Could not reach Google Drive. Your existing downloads are still available offline.".into()),
        }
        tokio::time::sleep(Duration::from_secs(1 << attempt)).await;
    }
    unreachable!()
}

pub async fn metadata(session: &mut Session, id: &str) -> Result<DriveFile, String> {
    get(
        session,
        file_url(session, id)?,
        &[("fields", FIELDS), ("supportsAllDrives", "true")],
        false,
    )
    .await?
    .json()
    .await
    .map_err(|_| "Google returned invalid file metadata.".into())
}

pub async fn folder(session: &mut Session, id: &str) -> Result<DriveFile, String> {
    let file = metadata(session, id).await?;
    if file.mime_type != FOLDER_MIME || file.trashed {
        return Err("Choose an existing Google Drive folder.".into());
    }
    Ok(file)
}

pub async fn list_comics(session: &mut Session, root: &str) -> Result<Vec<DriveFile>, String> {
    let mut pending = vec![(root.to_string(), Vec::<String>::new())];
    let mut visited = HashSet::new();
    let mut files = Vec::new();
    while let Some((parent, folder_path)) = pending.pop() {
        if !visited.insert(parent.clone()) {
            continue;
        }
        let escaped = parent.replace('\\', "\\\\").replace('\'', "\\'");
        let query = format!("'{escaped}' in parents and trashed = false");
        let fields = format!("nextPageToken,incompleteSearch,files({FIELDS})");
        let mut page_token = String::new();
        let mut pages = HashSet::new();
        loop {
            let page: FileList = get(
                session,
                session.files_url.clone(),
                &[
                    ("q", &query),
                    ("fields", &fields),
                    ("pageSize", "1000"),
                    ("pageToken", &page_token),
                    ("spaces", "drive"),
                    ("supportsAllDrives", "true"),
                    ("includeItemsFromAllDrives", "true"),
                ],
                false,
            )
            .await?
            .json()
            .await
            .map_err(|_| "Google returned an invalid folder listing.".to_string())?;
            if page.incomplete_search {
                return Err("Google returned an incomplete folder listing. Try syncing again; your library has been kept.".into());
            }
            for mut file in page.files {
                if file.trashed {
                    continue;
                }
                if file.mime_type == FOLDER_MIME {
                    let mut nested_path = folder_path.clone();
                    nested_path.push(file.name);
                    pending.push((file.id, nested_path));
                } else if file.is_comic() {
                    file.folder_path = folder_path.clone();
                    files.push(file);
                }
            }
            match page.next_page_token.filter(|t| !t.is_empty()) {
                Some(token) if pages.insert(token.clone()) => page_token = token,
                Some(_) => return Err("Google repeated a folder page. Try syncing again.".into()),
                None => break,
            }
        }
    }
    files.sort_by(|a, b| a.id.cmp(&b.id));
    files.dedup_by(|a, b| a.id == b.id);
    Ok(files)
}

pub async fn download(session: &mut Session, file: &DriveFile, path: &Path) -> Result<(), String> {
    let expected = file.file_size()?;
    if tokio::fs::metadata(path)
        .await
        .is_ok_and(|m| m.len() == expected)
    {
        return Ok(());
    }
    let parent = path.parent().ok_or("Invalid download path.".to_string())?;
    let temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|e| format!("Could not create a download: {e}"))?;
    let mut output = tokio::fs::File::from_std(temporary.reopen().map_err(|e| e.to_string())?);
    let mut response = get(
        session,
        file_url(session, &file.id)?,
        &[("alt", "media"), ("supportsAllDrives", "true")],
        true,
    )
    .await?;
    let mut received = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "A Drive download was interrupted. Sync again to retry.".to_string())?
    {
        received += chunk.len() as u64;
        if received > expected {
            return Err(format!(
                "{} changed during download. Sync again.",
                file.name
            ));
        }
        output
            .write_all(&chunk)
            .await
            .map_err(|e| format!("Could not save a Drive download: {e}"))?;
    }
    if received != expected {
        return Err(format!(
            "The download of {} was incomplete. Sync again.",
            file.name
        ));
    }
    if metadata(session, &file.id).await?.version != file.version {
        return Err(format!(
            "{} changed during download. Sync again.",
            file.name
        ));
    }
    output.flush().await.map_err(|e| e.to_string())?;
    output.sync_all().await.map_err(|e| e.to_string())?;
    drop(output);
    temporary
        .persist(path)
        .map_err(|e| format!("Could not finish saving a Drive download: {}", e.error))?;
    Ok(())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use tokio::io::AsyncReadExt;

    pub(crate) async fn server(
        responses: Vec<String>,
    ) -> (Session, tokio::task::JoinHandle<Vec<Url>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let task = tokio::spawn(async move {
            let mut requests = Vec::new();
            for body in responses {
                let (mut socket, _) =
                    tokio::time::timeout(Duration::from_secs(5), listener.accept())
                        .await
                        .unwrap()
                        .unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buffer = [0; 1024];
                    let n = socket.read(&mut buffer).await.unwrap();
                    assert!(n > 0);
                    request.extend_from_slice(&buffer[..n]);
                    if request.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8(request).unwrap();
                assert!(request
                    .to_lowercase()
                    .contains("authorization: bearer test-token"));
                let target = request.split_whitespace().nth(1).unwrap();
                requests.push(Url::parse(&format!("http://{address}{target}")).unwrap());
                let response = format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                socket.write_all(response.as_bytes()).await.unwrap();
            }
            requests
        });
        (
            Session::for_test(Url::parse(&format!("http://{address}/files")).unwrap()),
            task,
        )
    }

    fn comic_file() -> DriveFile {
        serde_json::from_value(serde_json::json!({
            "id": "comic", "name": "Saga.cbz", "mimeType": "application/zip", "version": "1", "size": "4"
        })).unwrap()
    }

    #[tokio::test]
    async fn recursively_lists_all_pages_and_skips_unsupported_items() {
        let (mut session, task) = server(vec![
            serde_json::json!({"nextPageToken":"page-two", "files":[
                {"id":"subfolder", "name":"Manga", "mimeType":FOLDER_MIME},
                {"id":"notes", "name":"notes.txt", "mimeType":"text/plain"}
            ]}).to_string(),
            serde_json::json!({"files":[
                {"id":"a", "name":"Saga.CBZ", "mimeType":"application/zip", "version":"1", "size":"4"},
                {"id":"shortcut", "name":"Alias.cbz", "mimeType":"application/vnd.google-apps.shortcut"}
            ]}).to_string(),
            serde_json::json!({"files":[
                {"id":"b", "name":"Manga.pdf", "mimeType":"application/pdf", "version":"2", "size":"8"}
            ]}).to_string(),
        ]).await;
        let files = list_comics(&mut session, "root").await.unwrap();
        assert_eq!(
            files.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert!(files[0].folder_path.is_empty());
        assert_eq!(files[1].folder_path, ["Manga"]);
        let requests = task.await.unwrap();
        assert!(requests[1]
            .query_pairs()
            .any(|(k, v)| k == "pageToken" && v == "page-two"));
        assert!(requests[2]
            .query_pairs()
            .any(|(k, v)| k == "q" && v == "'subfolder' in parents and trashed = false"));
        assert!(requests.iter().all(|r| r
            .query_pairs()
            .any(|(k, v)| k == "includeItemsFromAllDrives" && v == "true")));
    }

    #[tokio::test]
    async fn incomplete_listings_are_errors_instead_of_empty_collections() {
        let (mut session, task) =
            server(vec![r#"{"files":[],"incompleteSearch":true}"#.into()]).await;
        assert!(list_comics(&mut session, "root")
            .await
            .unwrap_err()
            .contains("incomplete"));
        task.await.unwrap();
    }

    #[tokio::test]
    async fn completed_downloads_are_atomic_and_reused() {
        let (mut session, task) = server(vec![
            "book".into(),
            r#"{"id":"comic","name":"Saga.cbz","mimeType":"application/zip","version":"1","size":"4"}"#.into(),
        ]).await;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("comic.cbz");
        download(&mut session, &comic_file(), &path).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"book");
        let requests = task.await.unwrap();
        assert!(requests[0]
            .query_pairs()
            .any(|(k, v)| k == "alt" && v == "media"));
        // The server has closed; this call must reuse the verified local copy.
        download(&mut session, &comic_file(), &path).await.unwrap();
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 1);
    }

    #[tokio::test]
    async fn truncated_downloads_leave_no_partial_archive() {
        let (mut session, task) = server(vec!["bo".into()]).await;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("comic.cbz");
        assert!(download(&mut session, &comic_file(), &path)
            .await
            .unwrap_err()
            .contains("incomplete"));
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(temp.path()).unwrap().count(), 0);
        task.await.unwrap();
    }

    #[tokio::test]
    async fn revisions_changed_during_download_are_not_published() {
        let (mut session, task) = server(vec![
            "book".into(),
            r#"{"id":"comic","name":"Saga.cbz","mimeType":"application/zip","version":"2","size":"4"}"#.into(),
        ]).await;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("comic.cbz");
        assert!(download(&mut session, &comic_file(), &path)
            .await
            .unwrap_err()
            .contains("changed"));
        assert!(!path.exists());
        task.await.unwrap();
    }

    #[test]
    fn accepts_folder_links_and_ids_without_allowing_query_injection() {
        assert_eq!(
            folder_id(" https://drive.google.com/drive/u/0/folders/abc_123-xyz?usp=sharing ")
                .unwrap(),
            "abc_123-xyz"
        );
        assert_eq!(folder_id("abc_123-xyz").unwrap(), "abc_123-xyz");
        for bad in [
            "",
            "../escape",
            "x' or trashed = false",
            "https://evil.example/folders/abc",
            "https://drive.google.com/file/d/abc/view",
        ] {
            assert!(folder_id(bad).is_err(), "{bad}");
        }
    }
}
