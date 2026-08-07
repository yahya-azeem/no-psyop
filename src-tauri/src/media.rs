use std::time::Duration;

use base64::Engine;
use reqwest::header::{CONTENT_TYPE, COOKIE, REFERER, USER_AGENT};

use crate::store::SecureStore;
use crate::types::Platform;

const UA: &str = "Mozilla/5.0 (Linux; Android 14; Pixel 8 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.6613.146 Mobile Safari/537.36";

const REFERER_URL: &str = "https://www.instagram.com/";

pub fn encode_url(url: &str) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(url.as_bytes())
}

pub fn decode_url(s: &str) -> Option<String> {
    if s.is_empty() {
        return None;
    }
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s).ok()?;
    String::from_utf8(bytes).ok()
}

pub fn fetch_media(
    store: &SecureStore,
    remote_url: &str,
) -> Result<(Vec<u8>, String), String> {
    let cred = store.get_credential(&Platform::Instagram).ok().flatten();
    let cookies = cred.map(|c| c.session_token).unwrap_or_default();

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|e| format!("client: {}", e))?;

    let mut req = client
        .get(remote_url)
        .header(USER_AGENT, UA)
        .header(REFERER, REFERER_URL);
    if !cookies.is_empty() {
        req = req.header(COOKIE, cookies);
    }

    let resp = req.send().map_err(|e| format!("origin: {}", e))?;
    let content_type = resp
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = resp.bytes().map_err(|e| format!("body: {}", e))?.to_vec();
    Ok((body, content_type))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_roundtrip() {
        let url = "https://scontent.cdninstagram.com/v/t50.2886/abc.mp4?ig_cache_key=123&se=7";
        let encoded = encode_url(url);
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
        assert!(!encoded.contains('='));
        assert_eq!(decode_url(&encoded).as_deref(), Some(url));
    }

    #[test]
    fn test_decode_invalid() {
        assert!(decode_url("!!not-base64!!").is_none());
        assert!(decode_url("").is_none());
    }
}
