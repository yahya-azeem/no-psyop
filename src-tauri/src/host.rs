use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::graph::GraphEngine;
use crate::ingestion::PlatformIngester;
use crate::ml::MLPipeline;
use crate::store::SecureStore;
use crate::types;

const ALL_PLATFORMS: [types::Platform; 3] = [
    types::Platform::Instagram,
    types::Platform::Twitter,
    types::Platform::LinkedIn,
];

/// Keep the host sync cadence independent from the GUI cooldowns.
const SYNC_COOLDOWN_SECS: u64 = 5 * 60;
const NEWS_TTL_SECS: u64 = 5 * 60;

fn parse_platform(s: &str) -> Option<types::Platform> {
    ALL_PLATFORMS
        .iter()
        .find(|p| format!("{:?}", p) == s)
        .cloned()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn was_refreshed(map: &Mutex<HashMap<types::Platform, u64>>, p: &types::Platform, now: u64) -> bool {
    if let Ok(guard) = map.lock() {
        if let Some(ts) = guard.get(p) {
            if now.saturating_sub(*ts) < SYNC_COOLDOWN_SECS {
                return true;
            }
        }
    }
    false
}

fn touch_sync(map: &Mutex<HashMap<types::Platform, u64>>, p: &types::Platform, now: u64) {
    if let Ok(mut guard) = map.lock() {
        guard.insert(p.clone(), now);
    }
}

struct HostState {
    graph: Mutex<GraphEngine>,
    ml: MLPipeline,
    store: SecureStore,
    news_cache: Mutex<Option<(u64, Vec<types::Post>)>>,
    feed_sync: Mutex<HashMap<types::Platform, u64>>,
    inbox_sync: Mutex<HashMap<types::Platform, u64>>,
    media_addr: String,
    rt: tokio::runtime::Runtime,
}

/// Run the headless sync host: a long-lived process that keeps the graph DB
/// fresh and serves a tiny JSON read API on `bind`. Media is served by the
/// existing media server on `media_bind`. Returns Ok(()) only if it starts;
/// the serving loop runs until the process is killed.
pub fn run_sync_host(bind: &str, media_bind: &str, interval_secs: u64) -> Result<(), String> {
    let data_dir = dirs_next::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let db_dir = data_dir.join("no_pysop");
    std::fs::create_dir_all(&db_dir).map_err(|e| e.to_string())?;
    let db_path = db_dir.join("graph.db");
    let db_path_str = db_path.to_string_lossy().to_string();
    let graph = GraphEngine::new(&db_path_str)
        .or_else(|_| GraphEngine::new(":memory:"))
        .map_err(|e| e.to_string())?;

    let media_addr: std::net::SocketAddr = media_bind
        .parse()
        .map_err(|e| format!("bad media addr {media_bind}: {e}"))?;
    let media_store = SecureStore::new();
    let media_cache = crate::media_server::MediaCache::new();
    {
        let s = media_store.clone();
        let c = media_cache.clone();
        std::thread::spawn(move || crate::media_server::serve(media_addr, s, c));
    }

    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let state = Arc::new(HostState {
        graph: Mutex::new(graph),
        ml: MLPipeline::new(),
        store: SecureStore::new(),
        news_cache: Mutex::new(None),
        feed_sync: Mutex::new(HashMap::new()),
        inbox_sync: Mutex::new(HashMap::new()),
        media_addr: media_bind.to_string(),
        rt,
    });

    if interval_secs > 0 {
        let s = state.clone();
        std::thread::spawn(move || background_loop(s, interval_secs));
    }

    let server = tiny_http::Server::http(bind).map_err(|e| format!("http bind {bind}: {e}"))?;
    eprintln!("[sync_host] listening on http://{bind}");
    for request in server.incoming_requests() {
        let s = state.clone();
        std::thread::spawn(move || handle(s, request));
    }
    Ok(())
}

fn background_loop(state: Arc<HostState>, interval_secs: u64) {
    loop {
        if let Err(e) = sync_all_inner(&state, false) {
            eprintln!("[sync_host] background sync: {e}");
        }
        std::thread::sleep(std::time::Duration::from_secs(interval_secs));
    }
}

type DynResp = tiny_http::Response<std::io::Cursor<Vec<u8>>>;

fn json_header() -> Option<tiny_http::Header> {
    tiny_http::Header::from_bytes(&b"Content-Type"[..], &b"application/json"[..]).ok()
}

fn resp(bytes: Vec<u8>, code: u16, header: Option<tiny_http::Header>) -> DynResp {
    let mut r = tiny_http::Response::from_data(bytes).with_status_code(code);
    if let Some(h) = header {
        r.add_header(h);
    }
    r
}

fn json_body(data: &str, code: u16) -> DynResp {
    resp(data.as_bytes().to_vec(), code, json_header())
}

fn ok_json<T: serde::Serialize>(v: &T) -> DynResp {
    match serde_json::to_vec(v) {
        Ok(b) => resp(b, 200, json_header()),
        Err(e) => json_body(&format!("{{\"error\":\"{e}\"}}"), 500),
    }
}

fn err_json(code: u16, msg: &str) -> DynResp {
    json_body(&format!("{{\"error\":{}}}", serde_json::json!(msg)), code)
}

fn parse_query(url: &str) -> (String, HashMap<String, String>) {
    let mut q = HashMap::new();
    match url.split_once('?') {
        Some((path, query)) => {
            for pair in query.split('&') {
                let mut it = pair.splitn(2, '=');
                if let (Some(k), Some(v)) = (it.next(), it.next()) {
                    q.insert(percent_decode(k), percent_decode(v));
                }
            }
            (path.to_string(), q)
        }
        None => (url.to_string(), q),
    }
}

fn percent_decode(s: &str) -> String {
    urlencoding::decode(s)
        .unwrap_or_else(|_| std::borrow::Cow::Borrowed(s))
        .into_owned()
}

fn media_bytes(b: Vec<u8>, ctype: String) -> DynResp {
    let h = tiny_http::Header::from_bytes(&b"Content-Type"[..], ctype.as_bytes())
        .ok()
        .or_else(json_header);
    resp(b, 200, h)
}

fn handle(state: Arc<HostState>, request: tiny_http::Request) {
    let url = request.url().to_string();
    let (path, q) = parse_query(&url);
    let response = route(&state, &path, &q);
    let _ = request.respond(response);
}

fn route(state: &Arc<HostState>, path: &str, q: &HashMap<String, String>) -> DynResp {
    // Health
    if path == "/health" {
        return json_body("{\"ok\":true}", 200);
    }

    // Media proxy: forward to the media server so a client only needs this port.
    if let Some(rest) = path.strip_prefix("/media/") {
        let media = format!("http://{}/{}", state.media_addr, rest);
        if let Ok(resp) = state.rt.block_on(async {
            let client = reqwest::Client::new();
            client.get(media).send().await
        }) {
            if resp.status().is_success() {
                let ctype = resp
                    .headers()
                    .get(reqwest::header::CONTENT_TYPE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("application/octet-stream")
                    .to_string();
                if let Ok(bytes) = state.rt.block_on(resp.bytes()) {
                    return media_bytes(bytes.to_vec(), ctype);
                }
            }
        }
        return err_json(404, "media not found");
    }

    // Data endpoints
    match path {
        "/feed" => {
            let platform = q.get("platform").map(|s| s.as_str()).unwrap_or("All");
            let uid = q.get("user_id").cloned().unwrap_or_default();
            match feed_inner(state, platform, &uid) {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(500, &e),
            }
        }
        "/news" => match news_inner(state) {
            Ok(v) => ok_json(&v),
            Err(e) => err_json(500, &e),
        },
        "/conversations" => {
            let all = state
                .graph
                .lock()
                .map_err(|e| e.to_string())
                .and_then(|g| g.get_all_conversations());
            match all {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(500, &e),
            }
        }
        "/messages" => {
            let cid = q.get("conversation_id").cloned().unwrap_or_default();
            let platform = q.get("platform").cloned().unwrap_or_default();
            let p = match parse_platform(&platform) {
                Some(p) => p,
                None => return err_json(400, "bad platform"),
            };
            match state.graph.lock()
                .map_err(|e| e.to_string())
                .and_then(|g| g.get_messages(&cid, &p)) {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(500, &e),
            }
        }
        "/search" => {
            let query = q.get("query").cloned().unwrap_or_default();
            let platform = q.get("platform").and_then(|s| parse_platform(s.as_str()));
            match state.graph.lock()
                .map_err(|e| e.to_string())
                .and_then(|g| g.search_posts_text(&query, platform.as_ref(), 50)) {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(500, &e),
            }
        }
        "/stories" => match stories_inner(state) {
            Ok(v) => ok_json(&v),
            Err(e) => err_json(500, &e),
        },
        "/credentials" => {
            let mut creds = Vec::new();
            for p in &ALL_PLATFORMS {
                if let Ok(Some(c)) = state.store.get_credential(p) {
                    creds.push(c);
                }
            }
            ok_json(&creds)
        }
        "/sync" => {
            let force = q.get("force").map(|s| s == "1" || s == "true").unwrap_or(false);
            match sync_all_inner(state, force) {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(500, &e),
            }
        }
        "/sync-inbox" => {
            let platform = q.get("platform").cloned().unwrap_or_default();
            let force = q.get("force").map(|s| s == "1" || s == "true").unwrap_or(false);
            let p = match parse_platform(&platform) {
                Some(p) => p,
                None => return err_json(400, "bad platform"),
            };
            match sync_inbox_inner(state, &p, force) {
                Ok(n) => ok_json(&n),
                Err(e) => err_json(500, &e),
            }
        }
        "/profile" => {
            let platform = q.get("platform").cloned().unwrap_or_default();
            let username = q.get("username").cloned().unwrap_or_default();
            match profile_inner(state, &platform, &username) {
                Ok(v) => ok_json(&v),
                Err(e) => err_json(500, &e),
            }
        }
        "/seen" => {
            let platform = q.get("platform").cloned().unwrap_or_default();
            let post_id = q.get("post_id").cloned().unwrap_or_default();
            let p = match parse_platform(&platform) {
                Some(p) => p,
                None => return err_json(400, "bad platform"),
            };
            match state
                .graph
                .lock()
                .map_err(|e| e.to_string())
                .and_then(|g| g.mark_post_seen(&p, &post_id))
            {
                Ok(()) => json_body("{\"ok\":true}", 200),
                Err(e) => err_json(500, &e),
            }
        }
        _ => err_json(404, "not found"),
    }
}

fn feed_inner(state: &Arc<HostState>, platform: &str, uid: &str) -> Result<Vec<types::FeedItem>, String> {
    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    let ml = &state.ml;

    let wrap = |p: types::Post| -> types::FeedItem {
        let result = ml.filter_post(&p);
        let prox = if !uid.is_empty() {
            graph.proximity_score(&uid.into(), &p).unwrap_or(0.0)
        } else {
            0.0
        };
        types::FeedItem {
            proximity_score: prox,
            relevance_score: if result.is_quality() { 1.0 } else { 0.0 },
            post: p,
        }
    };

    if let Some(p) = parse_platform(platform) {
        graph.get_feed(&p, 20).map(|v| v.into_iter().map(wrap).collect())
    } else {
        let mut all: Vec<types::FeedItem> = Vec::new();
        for p in &ALL_PLATFORMS {
            if let Ok(v) = graph.get_feed(p, 20) {
                all.extend(v.into_iter().map(wrap));
            }
        }
        all.sort_by(|a, b| b.post.timestamp.cmp(&a.post.timestamp));
        Ok(all)
    }
}

fn news_inner(state: &Arc<HostState>) -> Result<Vec<types::Post>, String> {
    let now = now_secs();
    if let Ok(guard) = state.news_cache.try_lock() {
        if let Some((ts, posts)) = guard.as_ref() {
            if now.saturating_sub(*ts) < NEWS_TTL_SECS {
                return Ok(posts.clone());
            }
        }
    }
    let cred = state
        .store
        .get_credential(&types::Platform::Twitter)?
        .ok_or_else(|| "Twitter not connected.".to_string())?;
    let posts = state
        .rt
        .block_on(ingestion_twitter_news(&cred))?;
    if let Ok(mut guard) = state.news_cache.try_lock() {
        *guard = Some((now, posts.clone()));
    }
    Ok(posts)
}

async fn ingestion_twitter_news(cred: &types::Credential) -> Result<Vec<types::Post>, String> {
    crate::ingestion::twitter::TwitterIngester
        .fetch_news(cred, &["Polymarket", "AJEnglish"])
        .await
}

fn stories_inner(state: &Arc<HostState>) -> Result<Vec<types::StoryUser>, String> {
    let cred = state
        .store
        .get_credential(&types::Platform::Instagram)?
        .ok_or_else(|| "Instagram not connected".to_string())?;
    let mut ing = crate::ingestion::instagram::InstagramIngester;
    state.rt.block_on(ing.fetch_stories(&cred))
}

fn profile_inner(state: &Arc<HostState>, platform: &str, username: &str) -> Result<types::SocialUser, String> {
    let p = match platform {
        "Instagram" => types::Platform::Instagram,
        "Twitter" => types::Platform::Twitter,
        "LinkedIn" => types::Platform::LinkedIn,
        _ => return Err(format!("unknown platform: {platform}")),
    };
    let cred = state.store.get_credential(&p)?.ok_or_else(|| "no credential".to_string())?;
    let mut ing: Box<dyn PlatformIngester + Send + Sync> = match p {
        types::Platform::Instagram => Box::new(crate::ingestion::instagram::InstagramIngester),
        types::Platform::Twitter => Box::new(crate::ingestion::twitter::TwitterIngester),
        types::Platform::LinkedIn => Box::new(crate::ingestion::linkedin::LinkedInIngester),
        types::Platform::Rss => return Err("RSS has no profiles".to_string()),
    };
    let profile = state
        .rt
        .block_on(async move { ing.fetch_profile(&cred, username).await })?;
    state.graph.lock().map_err(|e| e.to_string())?.sync_user(profile.clone())?;
    Ok(profile)
}

fn sync_all_inner(state: &Arc<HostState>, force: bool) -> Result<types::SyncResult, String> {
    let now = now_secs();

    let creds = {
        let mut all = Vec::new();
        for p in &ALL_PLATFORMS {
            if was_refreshed(&state.feed_sync, p, now) && !force {
                continue;
            }
            if let Ok(Some(cred)) = state.store.get_credential(p) {
                all.push(cred);
            }
        }
        all
    };

    let mut engine = crate::ingestion::IngestionEngine::new();
    let feed_results = state.rt.block_on(async { engine.fetch_all_feeds(&creds).await });
    let mut posts_added = 0usize;
    let mut errors = Vec::new();
    for (platform, result) in &feed_results {
        touch_sync(&state.feed_sync, platform, now);
        match result {
            Ok(posts) => {
                if let Ok(graph) = state.graph.lock() {
                    for post in posts {
                        if let Err(e) = graph.save_post(post) {
                            errors.push(format!("{:?}: save post {}: {}", platform, post.id, e));
                        } else {
                            posts_added += 1;
                        }
                    }
                }
            }
            Err(e) => errors.push(format!("{:?}: {}", platform, e)),
        }
    }

    let inbox_results = state.rt.block_on(async { engine.fetch_all_inboxes(&creds).await });
    let mut messages_added = 0usize;
    for (platform, result) in &inbox_results {
        touch_sync(&state.inbox_sync, platform, now);
        match result {
            Ok(threads) => {
                if let Ok(graph) = state.graph.lock() {
                    for (conv, msgs) in threads {
                        if let Err(e) = graph.save_conversation(conv) {
                            errors.push(format!("{:?}: save conversation {}: {}", platform, conv.id, e));
                        }
                        for m in msgs {
                            if let Err(e) = graph.save_message(m) {
                                errors.push(format!("{:?}: save message {}: {}", platform, m.id, e));
                            } else {
                                messages_added += 1;
                            }
                        }
                    }
                }
            }
            Err(e) => errors.push(format!("{:?} inbox: {}", platform, e)),
        }
    }

    Ok(types::SyncResult {
        posts_added,
        messages_added,
        errors,
    })
}

fn sync_inbox_inner(state: &Arc<HostState>, platform: &types::Platform, force: bool) -> Result<usize, String> {
    if !force && was_refreshed(&state.inbox_sync, platform, now_secs()) {
        return Ok(0);
    }
    let cred = state.store.get_credential(platform)?.ok_or_else(|| format!("{:?} not connected", platform))?;
    let ing: Box<dyn crate::ingestion::PlatformIngester + Send + Sync> = match platform {
        types::Platform::Instagram => Box::new(crate::ingestion::instagram::InstagramIngester),
        types::Platform::Twitter => Box::new(crate::ingestion::twitter::TwitterIngester),
        types::Platform::LinkedIn => Box::new(crate::ingestion::linkedin::LinkedInIngester),
        types::Platform::Rss => return Err("RSS has no inbox".to_string()),
    };
    let mut ing = ing;
    let threads = state
        .rt
        .block_on(async move { ing.fetch_inbox(&cred).await })?;
    touch_sync(&state.inbox_sync, platform, now_secs());
    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    let mut saved = 0usize;
    for (conv, msgs) in threads {
        graph.save_conversation(&conv)?;
        for m in msgs {
            graph.save_message(&m)?;
            saved += 1;
        }
    }
    Ok(saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Platform, Post};
    use std::io::Read;

    fn make_post(id: &str, platform: Platform, author: &str, content: &str, ts: u64) -> Post {
        Post {
            id: id.into(),
            platform,
            author_id: author.into(),
            author_username: author.into(),
            content: content.into(),
            media_urls: vec![],
            poster_url: None,
            liker_ids: vec![],
            commenter_ids: vec![],
            timestamp: ts,
            is_video: false,
            author_is_mutual: None,
            author_is_close_friend: None,
            engagement_score: None,
            is_synthetic: None,
            vector_embedding: None,
        }
    }

    fn test_state() -> Arc<HostState> {
        let graph = GraphEngine::new("").expect("in-memory graph");
        let dir = std::env::temp_dir().join(format!("no_pysop_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Arc::new(HostState {
            graph: Mutex::new(graph),
            ml: MLPipeline::new(),
            store: SecureStore::in_dir(dir),
            news_cache: Mutex::new(None),
            feed_sync: Mutex::new(HashMap::new()),
            inbox_sync: Mutex::new(HashMap::new()),
            media_addr: "127.0.0.1:8231".to_string(),
            rt: tokio::runtime::Runtime::new().expect("rt"),
        })
    }

    fn body(resp: DynResp) -> String {
        let mut s = String::new();
        resp.into_reader().read_to_string(&mut s).expect("read body");
        s
    }

    #[test]
    fn parse_platform_roundtrips() {
        assert_eq!(parse_platform("Twitter"), Some(Platform::Twitter));
        assert_eq!(parse_platform("Instagram"), Some(Platform::Instagram));
        assert_eq!(parse_platform("LinkedIn"), Some(Platform::LinkedIn));
        assert_eq!(parse_platform("nonsense"), None);
    }

    #[test]
    fn parse_query_splits_path_and_query() {
        let (path, q) = parse_query("/feed?platform=Twitter&user_id=user%201");
        assert_eq!(path, "/feed");
        assert_eq!(q.get("platform").map(|s| s.as_str()), Some("Twitter"));
        assert_eq!(q.get("user_id").map(|s| s.as_str()), Some("user 1"));
    }

    #[test]
    fn parse_query_no_query() {
        let (path, q) = parse_query("/health");
        assert_eq!(path, "/health");
        assert!(q.is_empty());
    }

    #[test]
    fn health_returns_ok() {
        let st = test_state();
        let body = body(route(&st, "/health", &HashMap::new()));
        assert_eq!(body, "{\"ok\":true}");
    }

    #[test]
    fn unknown_route_404() {
        let st = test_state();
        assert!(body(route(&st, "/nope", &HashMap::new())).contains("not found"));
    }

    #[test]
    fn feed_returns_all_then_filters_by_platform() {
        let st = test_state();
        {
            let g = st.graph.lock().unwrap();
            g.save_post(&make_post("p1", Platform::Twitter, "alice", "tweet one", 300)).unwrap();
            g.save_post(&make_post("p2", Platform::Instagram, "bob", "insta two", 200)).unwrap();
        }
        let all = body(route(&st, "/feed", &HashMap::new()));
        assert!(all.contains("alice") && all.contains("bob"));

        let q = HashMap::from([("platform".to_string(), "Twitter".to_string())]);
        let tw = body(route(&st, "/feed", &q));
        assert!(tw.contains("alice"));
        assert!(!tw.contains("bob"));
    }

    #[test]
    fn search_library_matches_text() {
        let st = test_state();
        {
            let g = st.graph.lock().unwrap();
            g.save_post(&make_post("s1", Platform::LinkedIn, "carol", "unique marketing insight", 100)).unwrap();
        }
        let q = HashMap::from([("query".to_string(), "marketing".to_string())]);
        let out = body(route(&st, "/search", &q));
        assert!(out.contains("carol"));
    }

    #[test]
    fn messages_empty_is_ok() {
        let st = test_state();
        let q = HashMap::from([
            ("conversation_id".to_string(), "c1".to_string()),
            ("platform".to_string(), "Twitter".to_string()),
        ]);
        assert_eq!(body(route(&st, "/messages", &q)), "[]");
    }

    #[test]
    fn sync_no_credentials_is_noop() {
        let st = test_state();
        let resp = route(&st, "/sync", &HashMap::new());
        let b = body(resp);
        assert!(b.contains("\"posts_added\":0"), "body={b}");
        assert!(b.contains("\"messages_added\":0"));
    }

    #[test]
    fn seen_marks_post() {
        let st = test_state();
        let q = HashMap::from([
            ("platform".to_string(), "Twitter".to_string()),
            ("post_id".to_string(), "p1".to_string()),
        ]);
        assert!(body(route(&st, "/seen", &q)).contains("\"ok\":true"));
    }

    #[test]
    fn news_without_twitter_cred_errors_gracefully() {
        let st = test_state();
        assert!(body(route(&st, "/news", &HashMap::new())).contains("error"));
    }

    #[test]
    fn stories_without_instagram_cred_errors_gracefully() {
        let st = test_state();
        let b = body(route(&st, "/stories", &HashMap::new()));
        assert!(b.contains("error"), "body={b}");
    }

    #[test]
    fn http_end_to_end_serves_feed() {
        let st = test_state();
        {
            let g = st.graph.lock().unwrap();
            g.save_post(&make_post("p1", Platform::Twitter, "alice", "over http", 500)).unwrap();
        }
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap().to_string();
        let url = format!("http://{addr}/feed?platform=Twitter");
        let (tx, rx) = std::sync::mpsc::channel();
        let client = std::thread::spawn(move || {
            let resp = reqwest::blocking::get(&url).unwrap();
            let _ = tx.send(resp.text().unwrap());
        });

        let request = server.recv().unwrap();
        let (path, q) = parse_query(request.url());
        let response = route(&st, &path, &q);
        let _ = request.respond(response);

        let got = rx.recv().unwrap();
        client.join().unwrap();
        assert!(got.contains("alice"));
        assert!(got.contains("over http"));
    }
}