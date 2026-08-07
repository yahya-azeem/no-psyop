use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::media;
use crate::store::SecureStore;

const MAX_CACHE_ITEMS: usize = 64;
const MAX_CACHE_BYTES: usize = 300 * 1024 * 1024;

#[derive(Clone)]
struct CachedMedia {
    bytes: Vec<u8>,
    content_type: String,
}

pub struct MediaCache {
    inner: Mutex<HashMap<String, CachedMedia>>,
    total_bytes: Mutex<usize>,
}

impl MediaCache {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(HashMap::new()),
            total_bytes: Mutex::new(0),
        })
    }
}

fn get_or_fetch(
    cache: &MediaCache,
    store: &SecureStore,
    url: &str,
) -> Result<(Vec<u8>, String), String> {
    {
        let inner = cache.inner.lock().map_err(|e| e.to_string())?;
        if let Some(m) = inner.get(url) {
            return Ok((m.bytes.clone(), m.content_type.clone()));
        }
    }

    let (bytes, content_type) = media::fetch_media(store, url)?;

    let mut inner = cache.inner.lock().map_err(|e| e.to_string())?;
    if let Ok(mut total) = cache.total_bytes.lock() {
        if inner.len() >= MAX_CACHE_ITEMS || *total + bytes.len() > MAX_CACHE_BYTES {
            for v in inner.values() {
                *total = total.saturating_sub(v.bytes.len());
            }
            inner.clear();
        }
        *total += bytes.len();
    }
    inner.insert(
        url.to_string(),
        CachedMedia {
            bytes: bytes.clone(),
            content_type: content_type.clone(),
        },
    );
    Ok((bytes, content_type))
}

fn serve_media(
    cache: &MediaCache,
    store: &SecureStore,
    url: &str,
    range: Option<&str>,
) -> (u16, String, Vec<u8>, Vec<(String, String)>) {
    let (bytes, content_type) = match get_or_fetch(cache, store, url) {
        Ok(v) => v,
        Err(e) => {
            return (
                502,
                "text/plain".into(),
                e.into_bytes(),
                vec![("Cache-Control".into(), "no-store".into())],
            )
        }
    };
    let ct = if content_type.is_empty() {
        if url.ends_with(".mp4") {
            "video/mp4".into()
        } else {
            "image/jpeg".into()
        }
    } else {
        content_type
    };

    let mut extra = vec![("Access-Control-Allow-Origin".into(), "*".into())];
    match parse_range(range, bytes.len()) {
        Some((start, end)) => {
            extra.push(("Content-Range".into(), format!("bytes {}-{}/{}", start, end, bytes.len())));
            let body = bytes[start..=end].to_vec();
            (206, ct, body, extra)
        }
        None if range.is_some() => (416, "text/plain".into(), Vec::new(), extra),
        None => (200, ct, bytes, extra),
    }
}

fn parse_range(range: Option<&str>, len: usize) -> Option<(usize, usize)> {
    if len == 0 {
        return None;
    }
    let range = range?.trim();
    let spec = range.strip_prefix("bytes=")?;
    let spec = spec.split(',').next()?.trim();
    if spec.starts_with('-') {
        let n: usize = spec[1..].trim().parse().ok()?;
        if n == 0 {
            return None;
        }
        let start = len.saturating_sub(n);
        return Some((start, len - 1));
    }
    if let Some((a, b)) = spec.split_once('-') {
        let start: usize = a.trim().parse().ok()?;
        if start >= len {
            return None;
        }
        let end = if b.trim().is_empty() {
            len - 1
        } else {
            let e: usize = b.trim().parse().ok()?;
            e.min(len - 1)
        };
        if start > end {
            return None;
        }
        Some((start, end))
    } else {
        None
    }
}

fn write_response(stream: &mut TcpStream, status: u16, ct: &str, body: &[u8], extra: &[(String, String)]) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccept-Ranges: bytes\r\nConnection: close\r\n",
        status_text(status),
        ct,
        body.len()
    );
    for (k, v) in extra {
        head.push_str(&format!("{}: {}\r\n", k, v));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn status_text(status: u16) -> String {
    match status {
        200 => "200 OK".into(),
        206 => "206 Partial Content".into(),
        404 => "404 Not Found".into(),
        416 => "416 Range Not Satisfiable".into(),
        _ => format!("{} Unknown", status),
    }
}

fn handle_conn(mut stream: TcpStream, store: &SecureStore, cache: &Arc<MediaCache>) -> std::io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(120)))?;

    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        let n = stream.read(&mut tmp)?;
        if n == 0 {
            return Ok(());
        }
        buf.extend_from_slice(&tmp[..n]);
        if buf.windows(4).any(|w| w == b"\r\n\r\n") || buf.len() > 64 * 1024 {
            break;
        }
    }

    let head = String::from_utf8_lossy(&buf);
    let mut lines = head.split("\r\n");
    let request_line = lines.next().unwrap_or("");
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let path = parts.next().unwrap_or("/");

    let mut range = None;
    for line in lines {
        if line.to_ascii_lowercase().starts_with("range:") {
            range = line.splitn(2, ':').nth(1).map(|s| s.trim().to_string());
        }
    }

    if method != "GET" && method != "HEAD" {
        return write_response(&mut stream, 405, "text/plain", b"method not allowed", &[]);
    }

    let remote = path.strip_prefix("/media/").and_then(|s| media::decode_url(s));
    let (status, ct, body, extra) = match remote {
        Some(url) => serve_media(cache, store, &url, range.as_deref()),
        None => (404, "text/plain".into(), Vec::from("not found"), vec![]),
    };
    let body = if method == "HEAD" { Vec::new() } else { body };
    write_response(&mut stream, status, &ct, &body, &extra)
}

/// Run the media server until the process exits.
///
/// Serves cached/streamed bytes over a real HTTP/1.1 connection so the
/// webview's media pipeline (WebKitGTK/GStreamer) gets proper Range/206
/// semantics instead of a single buffered custom-protocol response.
pub fn serve(addr: SocketAddr, store: SecureStore, cache: Arc<MediaCache>) {
    let listener = match TcpListener::bind(addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("media server bind {addr}: {e}");
            return;
        }
    };
    println!("[media_server] listening on {addr}");
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let store = store.clone();
                let cache = cache.clone();
                std::thread::spawn(move || {
                    let _ = handle_conn(s, &store, &cache);
                });
            }
            Err(_) => continue,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seeded_cache() -> Arc<MediaCache> {
        let cache = MediaCache::new();
        let mut inner = cache.inner.lock().unwrap();
        inner.insert(
            "https://cdn.example/video.mp4".into(),
            CachedMedia {
                bytes: vec![0u8; 1000],
                content_type: "video/mp4".into(),
            },
        );
        *cache.total_bytes.lock().unwrap() = 1000;
        drop(inner);
        cache
    }

    fn empty_store() -> SecureStore {
        // serve_media never fetches for cached URLs, so the store is unused.
        SecureStore::new()
    }

    #[test]
    fn range_parsing() {
        assert_eq!(parse_range(Some("bytes=0-"), 1000), Some((0, 999)));
        assert_eq!(parse_range(Some("bytes=0-1023"), 1000), Some((0, 999)));
        assert_eq!(parse_range(Some("bytes=100-199"), 1000), Some((100, 199)));
        assert_eq!(parse_range(Some("bytes=100-"), 1000), Some((100, 999)));
        assert_eq!(parse_range(Some("bytes=-500"), 1000), Some((500, 999)));
        assert_eq!(parse_range(Some("bytes=5000-"), 1000), None);
        assert_eq!(parse_range(Some("bytes=500-100"), 1000), None);
        assert_eq!(parse_range(None, 1000), None);
    }

    #[test]
    fn full_request_is_200_with_all_bytes() {
        let (status, ct, body, _extra) =
            serve_media(&seeded_cache(), &empty_store(), "https://cdn.example/video.mp4", None);
        assert_eq!(status, 200);
        assert_eq!(ct, "video/mp4");
        assert_eq!(body.len(), 1000);
    }

    #[test]
    fn range_request_is_206_with_content_range() {
        let (status, ct, body, extra) =
            serve_media(&seeded_cache(), &empty_store(), "https://cdn.example/video.mp4", Some("bytes=0-1023"));
        assert_eq!(status, 206);
        assert_eq!(ct, "video/mp4");
        assert_eq!(body.len(), 1000, "206 must carry only the requested range bytes");
        let cr = extra.iter().find(|(k, _)| k == "Content-Range").map(|(_, v)| v.as_str());
        assert_eq!(cr, Some("bytes 0-999/1000"));
    }

    #[test]
    fn suffix_range_request() {
        let (status, _ct, body, _extra) =
            serve_media(&seeded_cache(), &empty_store(), "https://cdn.example/video.mp4", Some("bytes=-100"));
        assert_eq!(status, 206);
        assert_eq!(body.len(), 100);
    }

    #[test]
    fn out_of_range_is_416() {
        let (status, _ct, _body, _extra) =
            serve_media(&seeded_cache(), &empty_store(), "https://cdn.example/video.mp4", Some("bytes=5000-"));
        assert_eq!(status, 416);
    }

    #[test]
    fn unknown_url_is_502() {
        let (status, _ct, _body, _extra) =
            serve_media(&seeded_cache(), &empty_store(), "https://cdn.example/missing.mp4", None);
        assert_eq!(status, 502);
    }
}
