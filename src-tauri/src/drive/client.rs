// Shared by the build script and backend tests. A Desktop OAuth client
// identifies the distributed app; it never contains a user's authorization.
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_secret: String,
}

impl OAuthClient {
    #[allow(dead_code)] // Used by build.rs and backend tests sharing this module.
    pub fn from_google_json(bytes: &[u8]) -> Result<Self, &'static str> {
        #[derive(Deserialize)]
        struct Download {
            installed: OAuthClient,
        }
        let client = serde_json::from_slice::<Download>(bytes)
            .map_err(|_| "Use the JSON downloaded for an OAuth Desktop app client.")?
            .installed;
        if !client.client_id.ends_with(".apps.googleusercontent.com")
            || client.client_id.bytes().any(|c| c.is_ascii_whitespace())
            || client.client_secret.trim().is_empty()
        {
            return Err("The Desktop OAuth client JSON is missing a valid client ID or secret.");
        }
        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_desktop_client_and_excludes_any_user_tokens() {
        let client = OAuthClient::from_google_json(
            br#"{"installed":{
            "client_id":"example.apps.googleusercontent.com", "client_secret":"desktop-client",
            "refresh_token":"must-not-be-bundled", "access_token":"must-not-be-bundled"
        }}"#,
        )
        .unwrap();
        let bundled = serde_json::to_value(client).unwrap();
        assert_eq!(bundled.as_object().unwrap().len(), 2);
        assert!(bundled.get("refresh_token").is_none());
        assert!(bundled.get("access_token").is_none());
    }

    #[test]
    fn rejects_web_clients_and_invalid_downloads() {
        for bytes in [
            br#"{"web":{"client_id":"example.apps.googleusercontent.com","client_secret":"s"}}"#.as_slice(),
            br#"{"installed":{"client_id":"invalid","client_secret":"s"}}"#,
            br#"{"installed":{"client_id":"example.apps.googleusercontent.com","client_secret":""}}"#,
            b"not json",
        ] {
            assert!(OAuthClient::from_google_json(bytes).is_err());
        }
    }
}
