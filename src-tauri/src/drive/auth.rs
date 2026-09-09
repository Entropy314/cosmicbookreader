use std::time::{Duration, Instant};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::{rngs::OsRng, RngCore};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub const SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

// Never send these fields over IPC or persist them in the library/config file.
#[derive(Clone, Serialize, Deserialize)]
pub struct Credentials {
    pub client_id: String,
    pub client_secret: String,
    #[serde(default)]
    pub refresh_token: String,
}

pub fn publisher_client() -> Option<super::client::OAuthClient> {
    serde_json::from_slice(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/google-drive-client.json"
    )))
    .expect("OAuth client was validated by the build script")
}

pub fn connection_credentials() -> Result<Credentials, String> {
    let client = publisher_client().ok_or("Google Drive is not available in this build. Contact the app publisher for a version with Drive enabled.".to_string())?;
    Ok(Credentials {
        client_id: client.client_id,
        client_secret: client.client_secret,
        refresh_token: String::new(),
    })
}

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new("com.entropy.cosmicbookreader", "google-drive")
        .map_err(|_| "Could not access the system credential store.".into())
}

pub async fn load() -> Result<Credentials, String> {
    let publisher = connection_credentials()?;
    tokio::task::spawn_blocking(move || {
        let secret = entry()?.get_password().map_err(|_| {
            "Unlock your system credential store, or reconnect Google Drive.".to_string()
        })?;
        let saved: Credentials =
            serde_json::from_str(&secret).map_err(|_| "Reconnect Google Drive.".to_string())?;
        restore_credentials(publisher, saved)
    })
    .await
    .map_err(|_| "Could not read Google credentials.".to_string())?
}

fn restore_credentials(
    mut publisher: Credentials,
    saved: Credentials,
) -> Result<Credentials, String> {
    if saved.client_id != publisher.client_id || saved.refresh_token.is_empty() {
        return Err("The app's Google connection has changed. Reconnect Google Drive.".into());
    }
    publisher.refresh_token = saved.refresh_token;
    Ok(publisher)
}

pub async fn save(credentials: Credentials) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let secret = serde_json::to_string(&credentials).map_err(|_| "Could not encode credentials.".to_string())?;
        entry()?.set_password(&secret)
            .map_err(|_| "Could not save Google credentials. Unlock your system credential store and try again.".into())
    }).await.map_err(|_| "Could not save Google credentials.".to_string())?
}

pub async fn forget() -> Result<(), String> {
    tokio::task::spawn_blocking(|| match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => {
            Err("Could not remove Google credentials from the system credential store.".into())
        }
    })
    .await
    .map_err(|_| "Could not remove Google credentials.".to_string())?
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: Option<String>,
    scope: Option<String>,
}

pub struct Session {
    pub client: Client,
    pub files_url: Url,
    credentials: Credentials,
    access_token: String,
    expires_at: Instant,
}

impl Session {
    pub fn new(client: Client, credentials: Credentials) -> Self {
        Self {
            client,
            credentials,
            access_token: String::new(),
            expires_at: Instant::now(),
            files_url: Url::parse("https://www.googleapis.com/drive/v3/files").unwrap(),
        }
    }

    #[cfg(test)]
    pub fn for_test(files_url: Url) -> Self {
        let mut session = Self::new(
            Client::new(),
            Credentials {
                client_id: String::new(),
                client_secret: String::new(),
                refresh_token: String::new(),
            },
        );
        session.files_url = files_url;
        session.access_token = "test-token".into();
        session.expires_at = Instant::now() + Duration::from_secs(3600);
        session
    }

    pub async fn token(&mut self) -> Result<String, String> {
        if Instant::now() >= self.expires_at {
            if self.credentials.refresh_token.is_empty() {
                return Err("Connect Google Drive to start syncing.".into());
            }
            let response = self
                .client
                .post(TOKEN_URL)
                .form(&[
                    ("client_id", self.credentials.client_id.as_str()),
                    ("client_secret", self.credentials.client_secret.as_str()),
                    ("refresh_token", self.credentials.refresh_token.as_str()),
                    ("grant_type", "refresh_token"),
                ])
                .send()
                .await
                .map_err(|_| {
                    "Could not reach Google. Check your internet connection.".to_string()
                })?;
            let token = parse_token(response).await?;
            self.access_token = token.access_token;
            self.expires_at =
                Instant::now() + Duration::from_secs(token.expires_in.saturating_sub(60));
        }
        Ok(self.access_token.clone())
    }
}

async fn parse_token(response: reqwest::Response) -> Result<TokenResponse, String> {
    if !response.status().is_success() {
        return Err(
            "Google authorization failed or expired. Reconnect Drive and grant read access.".into(),
        );
    }
    let token: TokenResponse = response
        .json()
        .await
        .map_err(|_| "Google returned an invalid authorization response.".to_string())?;
    if token
        .scope
        .as_ref()
        .is_some_and(|s| !s.split_whitespace().any(|v| v == SCOPE))
    {
        return Err("Google Drive read access was not granted. Reconnect and allow it.".into());
    }
    Ok(token)
}

fn random_secret() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn callback_code(target: &str, expected_state: &str) -> Result<String, String> {
    let url = Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| "Invalid sign-in callback.".to_string())?;
    if url.path() != "/oauth2callback" {
        return Err("Invalid sign-in callback.".into());
    }
    let values: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    if values.get("state").map(String::as_str) != Some(expected_state) {
        return Err("Sign-in verification failed. Try connecting again.".into());
    }
    if values.contains_key("error") {
        return Err("Google sign-in was cancelled or denied.".into());
    }
    values
        .get("code")
        .filter(|s| !s.is_empty())
        .cloned()
        .ok_or("Google did not return a sign-in code.".into())
}

pub async fn authorize(
    client: &Client,
    mut credentials: Credentials,
) -> Result<Credentials, String> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|_| "Could not open a local listener for Google sign-in.".to_string())?;
    let redirect = format!(
        "http://127.0.0.1:{}/oauth2callback",
        listener.local_addr().map_err(|e| e.to_string())?.port()
    );
    let state = random_secret();
    let verifier = random_secret();
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let mut url = Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
    url.query_pairs_mut().extend_pairs([
        ("client_id", credentials.client_id.as_str()),
        ("redirect_uri", &redirect),
        ("response_type", "code"),
        ("scope", SCOPE),
        ("state", &state),
        ("code_challenge", &challenge),
        ("code_challenge_method", "S256"),
        ("access_type", "offline"),
        ("prompt", "consent select_account"),
    ]);
    tokio::task::spawn_blocking(move || open::that(url.as_str()))
        .await
        .map_err(|_| "Could not open your browser.".to_string())?
        .map_err(|_| "Could not open your browser for Google sign-in.".to_string())?;

    let code = tokio::time::timeout(Duration::from_secs(180), async {
        loop {
            let (mut stream, _) = listener.accept().await.map_err(|e| e.to_string())?;
            let request = tokio::time::timeout(Duration::from_secs(5), async {
                let mut bytes = Vec::new();
                loop {
                    let mut buffer = [0; 1024];
                    let n = stream.read(&mut buffer).await.map_err(|e| e.to_string())?;
                    bytes.extend_from_slice(&buffer[..n]);
                    if bytes.windows(4).any(|w| w == b"\r\n\r\n") { break; }
                    if n == 0 || bytes.len() > 8192 { return Err("Invalid callback request.".to_string()); }
                }
                Ok(String::from_utf8_lossy(&bytes).to_string())
            }).await;
            let Ok(Ok(request)) = request else { continue; };
            let Some(target) = request.lines().next().and_then(|l| l.strip_prefix("GET ")).and_then(|l| l.split_whitespace().next()) else { continue; };
            if !target.starts_with("/oauth2callback?") { continue; }
            let result = callback_code(target, &state);
            let message = if result.is_ok() { "Sign-in received. Return to Cosmic Book Reader." } else { "Sign-in failed. Return to Cosmic Book Reader and try again." };
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{message}", message.len());
            let _ = tokio::time::timeout(Duration::from_secs(2), stream.write_all(response.as_bytes())).await;
            return result;
        }
    }).await.map_err(|_| "Google sign-in timed out. Try connecting again.".to_string())??;

    let response = client
        .post(TOKEN_URL)
        .form(&[
            ("client_id", credentials.client_id.as_str()),
            ("client_secret", &credentials.client_secret),
            ("code", &code),
            ("code_verifier", &verifier),
            ("redirect_uri", &redirect),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|_| {
            "Could not finish Google sign-in. Check your internet connection.".to_string()
        })?;
    credentials.refresh_token = parse_token(response)
        .await?
        .refresh_token
        .ok_or("Google did not grant offline access. Reconnect and allow access.".to_string())?;
    Ok(credentials)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_requires_matching_state_and_code() {
        assert_eq!(
            callback_code("/oauth2callback?state=expected&code=abc", "expected").unwrap(),
            "abc"
        );
        assert!(callback_code("/oauth2callback?state=wrong&code=abc", "expected").is_err());
        assert!(callback_code("/oauth2callback?code=abc", "expected").is_err());
        assert!(callback_code(
            "/oauth2callback?state=expected&error=access_denied",
            "expected"
        )
        .is_err());
        assert!(callback_code("/elsewhere?state=expected&code=abc", "expected").is_err());
    }

    #[test]
    fn restores_each_users_token_only_for_the_same_publisher_client() {
        let publisher = Credentials {
            client_id: "publisher.apps.googleusercontent.com".into(),
            client_secret: "current-desktop-client-value".into(),
            refresh_token: String::new(),
        };
        for token in ["user-a-token", "user-b-token"] {
            let saved = Credentials {
                client_secret: "old-desktop-client-value".into(),
                refresh_token: token.into(),
                ..publisher.clone()
            };
            let restored = restore_credentials(publisher.clone(), saved).unwrap();
            assert_eq!(restored.refresh_token, token);
            assert_eq!(restored.client_secret, publisher.client_secret);
        }
        let different_client = Credentials {
            client_id: "another.apps.googleusercontent.com".into(),
            refresh_token: "old-client-token".into(),
            ..publisher.clone()
        };
        assert!(restore_credentials(publisher.clone(), different_client).is_err());
        assert!(restore_credentials(publisher.clone(), publisher).is_err());
    }

    #[test]
    fn a_new_connection_never_starts_with_a_bundled_user_token() {
        match publisher_client() {
            Some(client) => {
                let credentials = connection_credentials().unwrap();
                assert_eq!(credentials.client_id, client.client_id);
                assert!(credentials.refresh_token.is_empty());
            }
            None => assert!(connection_credentials().is_err()),
        }
    }
}
