
pub mod types;
pub mod store;
mod graph;
pub mod http;
pub mod ingestion;
mod ml;
// Planned inbox-polling bridge; kept behind an allow until the messaging
// aggregation feature is wired to the UI.
#[allow(dead_code)]
mod bridge;
pub mod media;
mod media_server;
pub mod host;

use std::collections::HashMap;
use std::sync::Mutex;
use tauri::State;
use ingestion::PlatformIngester;

/// The three platforms this app ingests from.
const ALL_PLATFORMS: [types::Platform; 3] = [
    types::Platform::Instagram,
    types::Platform::Twitter,
    types::Platform::LinkedIn,
];

/// Map a user-facing platform string (as sent by the UI) to the typed enum.
fn parse_platform(s: &str) -> Option<types::Platform> {
    ALL_PLATFORMS
        .iter()
        .find(|p| format!("{:?}", p) == s)
        .cloned()
}

struct AppState {
    graph: Mutex<graph::GraphEngine>,
    ml: ml::MLPipeline,
    store: store::SecureStore,
    news_cache: Mutex<Option<(u64, Vec<types::Post>)>>,
    feed_sync: Mutex<HashMap<types::Platform, u64>>,
    inbox_sync: Mutex<HashMap<types::Platform, u64>>,
}

/// How long a news fetch is served from cache before hitting the network again.
const NEWS_TTL_SECS: u64 = 5 * 60;

/// Minimum gap between two background re-syncs of the same platform/source.
/// Startup syncs (non-forced) skip sources refreshed within this window so a
/// relaunch doesn't grind through every network call again.
const SYNC_COOLDOWN_SECS: u64 = 5 * 60;

fn touch_sync(map: &Mutex<HashMap<types::Platform, u64>>, p: &types::Platform, now: u64) {
    if let Ok(mut guard) = map.lock() {
        guard.insert(p.clone(), now);
    }
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

pub(crate) fn data_dir_path() -> std::path::PathBuf {
    dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("no_pysop")
}

pub(crate) fn news_cache_file() -> std::path::PathBuf {
    data_dir_path().join("news_cache.json")
}

pub(crate) fn load_news_disk() -> Option<(u64, Vec<types::Post>)> {
    let bytes = std::fs::read(news_cache_file()).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    let ts = v["ts"].as_u64()?;
    let posts: Vec<types::Post> = serde_json::from_value(v["posts"].clone()).ok()?;
    Some((ts, posts))
}

pub(crate) fn save_news_disk(ts: u64, posts: &[types::Post]) {
    let obj = serde_json::json!({ "ts": ts, "posts": posts });
    if let Ok(s) = serde_json::to_string(&obj) {
        let _ = std::fs::write(news_cache_file(), s);
    }
}

pub(crate) fn rss_sources_file() -> std::path::PathBuf {
    data_dir_path().join("rss_sources.json")
}

pub(crate) fn rss_sources_vec() -> Vec<String> {
    std::fs::read_to_string(rss_sources_file())
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}

pub(crate) fn default_rss_sources() -> Vec<String> {
    vec![
        "https://hnrss.org/frontpage".to_string(),
        "https://www.aljazeera.com/xml/rss/all.xml".to_string(),
    ]
}

#[tauri::command]
async fn get_news(state: State<'_, AppState>) -> Result<Vec<types::Post>, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();

    // Serve from the in-memory cache if fresh...
    if let Ok(guard) = state.news_cache.lock() {
        if let Some((ts, posts)) = guard.as_ref() {
            if now.saturating_sub(*ts) < NEWS_TTL_SECS {
                return Ok(posts.clone());
            }
        }
    }
    // ...otherwise fall back to the durable on-disk cache (survives restarts).
    if let Some((ts, posts)) = load_news_disk() {
        if now.saturating_sub(ts) < NEWS_TTL_SECS {
            if let Ok(mut guard) = state.news_cache.lock() {
                *guard = Some((ts, posts.clone()));
            }
            return Ok(posts);
        }
    }

    let cred = state.store.get_credential(&types::Platform::Twitter)
        .map_err(|e| e.to_string())?.ok_or_else(|| "Twitter not connected.".to_string())?;
    let posts = ingestion::twitter::TwitterIngester.fetch_news(&cred, &["Polymarket", "AJEnglish"]).await?;

    if let Ok(mut guard) = state.news_cache.lock() {
        *guard = Some((now, posts.clone()));
    }
    save_news_disk(now, &posts);
    Ok(posts)
}

#[tauri::command]
async fn get_feed(state: State<'_, AppState>, user_id: Option<String>, platform: Option<String>) -> Result<Vec<types::FeedItem>, String> {
    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    let ml = &state.ml;
    let platform = platform.as_deref().unwrap_or("All");
    let uid = user_id.as_deref().unwrap_or("");

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

    let feed = parse_platform(&platform).map(|p| graph.get_feed(&p, 20));
    match feed {
        Some(result) => result.map(|v| v.into_iter().map(wrap).collect()),
        None => {
            let mut all: Vec<types::FeedItem> = Vec::new();
            for p in &ALL_PLATFORMS {
                if let Ok(v) = graph.get_feed(p, 20) {
                    all.extend(v.into_iter().map(|p| wrap(p)));
                }
            }
            all.sort_by(|a, b| b.post.timestamp.cmp(&a.post.timestamp));
            Ok(all)
        }
    }
}

#[tauri::command]
async fn search_library(state: State<'_, AppState>, query: String, platform: Option<String>) -> Result<Vec<types::Post>, String> {
    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    let platform = platform.as_deref().and_then(|s| match s {
        "Instagram" => Some(types::Platform::Instagram),
        "Twitter" => Some(types::Platform::Twitter),
        "LinkedIn" => Some(types::Platform::LinkedIn),
        _ => None,
    });
    graph.search_posts_text(&query, platform.as_ref(), 50).map_err(|e| e.to_string())
}

#[tauri::command]
async fn search_platform(state: State<'_, AppState>, platform: String, query: String) -> Result<Vec<types::Post>, String> {
    let platform = match platform.as_str() {
        "Instagram" => types::Platform::Instagram,
        "Twitter" => types::Platform::Twitter,
        "LinkedIn" => types::Platform::LinkedIn,
        _ => return Err("unknown platform".into()),
    };

    let cred = state.store.get_credential(&platform)?
        .ok_or_else(|| format!("{:?} not connected", platform))?;

    match platform {
        types::Platform::Instagram => {
            let mut ing = ingestion::instagram::InstagramIngester;
            ing.search_posts(&cred, &query).await
        }
        types::Platform::Twitter => {
            let ing = ingestion::twitter::TwitterIngester;
            ing.search_posts(&cred, &query).await
        }
        // LinkedIn's public search API is unstable for live scraping; search
        // the local indexed library instead so the platform is still searchable.
        types::Platform::LinkedIn => {
            let graph = state.graph.lock().map_err(|e| e.to_string())?;
            graph.search_posts_text(&query, Some(&types::Platform::LinkedIn), 30).map_err(|e| e.to_string())
        }
        types::Platform::Rss => {
            let graph = state.graph.lock().map_err(|e| e.to_string())?;
            graph.search_posts_text(&query, Some(&types::Platform::Rss), 30).map_err(|e| e.to_string())
        }
    }
}

#[tauri::command]
fn store_credential(state: State<AppState>, platform: String, session_token: String, user_id: String) -> Result<(), String> {
    let p = parse_platform(&platform).ok_or_else(|| format!("unknown platform: {}", platform))?;
    let cred = types::Credential { platform: p, session_token, user_id };
    state.store.store_credential(&cred)
}

#[tauri::command]
async fn get_credentials(state: State<'_, AppState>) -> Result<Vec<types::Credential>, String> {
    let store = &state.store;
    let mut creds = Vec::new();
    for p in &ALL_PLATFORMS {
        if let Ok(Some(cred)) = store.get_credential(p) {
            creds.push(cred);
        }
    }
    Ok(creds)
}

#[tauri::command]
fn remove_credential(state: State<AppState>, platform: String) -> Result<(), String> {
    let p = parse_platform(&platform).ok_or_else(|| format!("unknown platform: {}", platform))?;
    state.store.remove_credential(&p)
}

/// Connect LinkedIn by opening the device-trusted headed profile once.
///
/// LinkedIn device-binds its session cookie, so a browser that has never been
/// used to sign in is rejected (429/redirect-loop). The sidecar opens a headed
/// window in a dedicated profile; after the user signs in it returns the fresh
/// session, which we persist as the LinkedIn credential.
#[tauri::command]
async fn linkedin_connect(state: State<'_, AppState>) -> Result<(), String> {
    let body = crate::http::xproxy::XProxy::op("", "linkedin_login", "").await?;
    let session_token = body["session_token"]
        .as_str()
        .ok_or_else(|| "linkedin login returned no session".to_string())?
        .to_string();
    let cred = types::Credential {
        platform: types::Platform::LinkedIn,
        session_token,
        user_id: "".into(),
    };
    state.store.store_credential(&cred)
}

#[tauri::command]
fn analyze_post(state: State<AppState>, content: String) -> Result<ml::PostFilterResult, String> {
    let post = types::Post {
        id: "".into(),
        platform: types::Platform::Twitter,
        author_id: "".into(),
        author_username: "".into(),
        content,
        media_urls: vec![],
        poster_url: None,
        liker_ids: vec![],
        commenter_ids: vec![],
        timestamp: 0,
        is_video: false,
        author_is_mutual: None,
        author_is_close_friend: None,
        engagement_score: None,
        is_synthetic: None,
        vector_embedding: None,
    };
    Ok(state.ml.filter_post(&post))
}

#[tauri::command]
async fn get_conversations(state: State<'_, AppState>, platform: Option<String>) -> Result<Vec<types::Conversation>, String> {
    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    match platform {
        Some(p) => {
            let p = parse_platform(&p).ok_or_else(|| format!("unknown platform: {}", p))?;
            graph.get_conversations(&p)
        }
        None => graph.get_all_conversations(),
    }
}

#[tauri::command]
async fn get_messages(state: State<'_, AppState>, conversation_id: String, platform: String) -> Result<Vec<types::Message>, String> {
    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    let p = parse_platform(&platform).ok_or_else(|| format!("unknown platform: {}", platform))?;
    graph.get_messages(&conversation_id, &p)
}

#[tauri::command]
async fn get_stories(state: State<'_, AppState>) -> Result<Vec<types::StoryUser>, String> {
    let cred = state.store.get_credential(&types::Platform::Instagram)?
        .ok_or_else(|| "Instagram not connected".to_string())?;
    let mut ing = ingestion::instagram::InstagramIngester;
    ing.fetch_stories(&cred).await
}

#[tauri::command]
async fn get_comments(state: State<'_, AppState>, media_id: String) -> Result<Vec<types::Comment>, String> {
    let cred = state.store.get_credential(&types::Platform::Instagram)?
        .ok_or_else(|| "Instagram not connected".to_string())?;
    let mut ing = ingestion::instagram::InstagramIngester;
    ing.fetch_comments(&cred, &media_id).await
}

/// Send a direct-message reply. The sent message is also persisted locally so
/// it appears instantly in the thread.
#[tauri::command]
async fn send_message(state: State<'_, AppState>, platform: String, conversation_id: String, content: String) -> Result<(), String> {
    let p = match platform.as_str() {
        "Instagram" => types::Platform::Instagram,
        "Twitter" => types::Platform::Twitter,
        "LinkedIn" => types::Platform::LinkedIn,
        _ => return Err(format!("unknown platform: {}", platform)),
    };
    let content = content.trim();
    if content.is_empty() {
        return Err("message is empty".to_string());
    }
    let cred = state.store.get_credential(&p)?.ok_or_else(|| format!("{:?} not connected", p))?;

    let mut ing: Box<dyn ingestion::PlatformIngester + Send + Sync> = match p {
        types::Platform::Instagram => Box::new(ingestion::instagram::InstagramIngester),
        types::Platform::Twitter => Box::new(ingestion::twitter::TwitterIngester),
        types::Platform::LinkedIn => Box::new(ingestion::linkedin::LinkedInIngester),
        types::Platform::Rss => return Err("RSS has no messaging".into()),
    };
    ing.send_message(&cred, &conversation_id, content).await?;

    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    graph.save_message(&types::Message {
        id: format!("local-{}-{}", p, now_secs()),
        platform: p.clone(),
        conversation_id: conversation_id.clone(),
        sender_id: "You".into(),
        content: content.to_string(),
        timestamp: now_secs(),
        is_mine: true,
    })?;
    Ok(())
}

/// Fetch configured RSS sources.
#[tauri::command]
async fn rss_get_sources() -> Result<Vec<String>, String> {
    let sources = rss_sources_vec();
    if sources.is_empty() {
        Ok(default_rss_sources())
    } else {
        Ok(sources)
    }
}

/// Persist the configured RSS sources.
#[tauri::command]
fn rss_set_sources(sources: Vec<String>) -> Result<(), String> {
    let cleaned: Vec<String> = sources
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    std::fs::create_dir_all(data_dir_path()).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(&cleaned).map_err(|e| e.to_string())?;
    std::fs::write(rss_sources_file(), json).map_err(|e| e.to_string())
}

/// Pull every configured RSS feed and store the parsed posts in the graph.
#[tauri::command]
async fn rss_sync(state: State<'_, AppState>) -> Result<usize, String> {
    let feeds = {
        let sources = rss_sources_vec();
        if sources.is_empty() {
            default_rss_sources()
        } else {
            sources
        }
    };
    if feeds.is_empty() {
        return Ok(0);
    }
    let posts = ingestion::rss::fetch_all(&feeds).await;
    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    let mut added = 0usize;
    for p in &posts {
        if graph.save_post(p).is_ok() {
            added += 1;
        }
    }
    Ok(added)
}

/// Read RSS-derived posts from the library (bottom grid section).
#[tauri::command]
async fn get_rss_posts(state: State<'_, AppState>) -> Result<Vec<types::Post>, String> {
    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    graph.get_feed(&types::Platform::Rss, 30)
}

#[tauri::command]
async fn sync_messages(state: State<'_, AppState>, platform: String, force: Option<bool>) -> Result<usize, String> {
    let force = force.unwrap_or(false);
    match parse_platform(&platform) {
        Some(p) => sync_platform_inbox(&state, &p, force).await,
        None if platform == "All" => {
            let mut total = 0usize;
            for pp in &ALL_PLATFORMS {
                if let Ok(Some(_)) = state.store.get_credential(pp) {
                    total += sync_platform_inbox(&state, pp, force).await?;
                }
            }
            Ok(total)
        }
        None => Err(format!("unknown platform: {}", platform)),
    }
}

async fn sync_platform_inbox(state: &State<'_, AppState>, platform: &types::Platform, force: bool) -> Result<usize, String> {
    if !force && was_refreshed(&state.inbox_sync, platform, now_secs()) {
        return Ok(0);
    }

    let cred = state.store.get_credential(platform)?
        .ok_or_else(|| format!("{:?} not connected", platform))?;

    let ing: Box<dyn ingestion::PlatformIngester + Send + Sync> = match platform {
        types::Platform::Instagram => Box::new(ingestion::instagram::InstagramIngester),
        types::Platform::Twitter => Box::new(ingestion::twitter::TwitterIngester),
        types::Platform::LinkedIn => Box::new(ingestion::linkedin::LinkedInIngester),
        types::Platform::Rss => return Err("RSS has no inbox".to_string()),
    };

    let mut ing = ing;
    let threads = ing.fetch_inbox(&cred).await?;
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

#[tauri::command]
async fn mark_post_seen(state: State<'_, AppState>, platform: String, post_id: String) -> Result<(), String> {
    let p = parse_platform(&platform).ok_or_else(|| format!("unknown platform: {}", platform))?;
    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    graph.mark_post_seen(&p, &post_id)
}

#[tauri::command]
async fn monitor_profile(state: State<'_, AppState>, platform: String, username: String) -> Result<types::SocialUser, String> {
    let p = match platform.as_str() {
        "Instagram" => types::Platform::Instagram,
        "Twitter" => types::Platform::Twitter,
        "LinkedIn" => types::Platform::LinkedIn,
        _ => return Err("unknown platform".into()),
    };
    let cred = state.store.get_credential(&p)?
        .ok_or_else(|| format!("No credential for {}", platform))?;

    let profile = match p {
        types::Platform::Instagram => {
            let mut ing = ingestion::instagram::InstagramIngester;
            ing.fetch_profile(&cred, &username).await?
        }
        types::Platform::Twitter => {
            let mut ing = ingestion::twitter::TwitterIngester;
            ing.fetch_profile(&cred, &username).await?
        }
        types::Platform::LinkedIn => {
            let mut ing = ingestion::linkedin::LinkedInIngester;
            ing.fetch_profile(&cred, &username).await?
        }
        types::Platform::Rss => return Err("RSS has no profiles".into()),
    };

    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    graph.sync_user(profile.clone())?;
    Ok(profile)
}

#[tauri::command]
async fn sync_all(state: State<'_, AppState>, force: Option<bool>) -> Result<types::SyncResult, String> {
    let force = force.unwrap_or(false);
    let now = now_secs();

    let creds = {
        let store = &state.store;
        let mut all = Vec::new();
        for p in &ALL_PLATFORMS {
            if was_refreshed(&state.feed_sync, p, now) && !force {
                continue;
            }
            if let Ok(Some(cred)) = store.get_credential(p) {
                all.push(cred);
            }
        }
        all
    };

    if creds.is_empty() {
        return Ok(types::SyncResult {
            posts_added: 0,
            messages_added: 0,
            errors: Vec::new(),
        });
    }

    let mut engine = ingestion::IngestionEngine::new();
    let results = engine.fetch_all_feeds(&creds).await;

    let mut posts_added = 0usize;
    let mut errors = Vec::new();

    for (platform, result) in &results {
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
            Err(e) => {
                errors.push(format!("{:?}: {}", platform, e));
            }
        }
    }

    let inbox_results = engine.fetch_all_inboxes(&creds).await;
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
            Err(e) => {
                errors.push(format!("{:?} inbox: {}", platform, e));
            }
        }
    }

    Ok(types::SyncResult {
        posts_added,
        messages_added,
        errors,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let data_dir = dirs_next::data_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let db_dir = data_dir.join("no_pysop");
    std::fs::create_dir_all(&db_dir).ok();
    let db_path = db_dir.join("graph.db");
    let db_path_str = db_path.to_string_lossy().to_string();
    let graph = graph::GraphEngine::new(&db_path_str).unwrap_or_else(|_| {
        graph::GraphEngine::new(":memory:").expect("failed to init graph")
    });

    let media_store = store::SecureStore::new();
    let media_cache = media_server::MediaCache::new();
    {
        let s = media_store.clone();
        let c = media_cache.clone();
        std::thread::spawn(move || {
            media_server::serve("127.0.0.1:8231".parse().expect("media addr"), s, c);
        });
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            use tauri::tray::{TrayIconBuilder, TrayIconEvent};
            use tauri::menu::{MenuBuilder, MenuItemBuilder};
            use tauri::Manager;

            let show = MenuItemBuilder::with_id("show", "Open no pysop").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app).items(&[&show, &quit]).build()?;

            TrayIconBuilder::with_id("no-pysop-tray")
                .tooltip("no pysop")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;
            Ok(())
        })
        .manage(AppState {
            graph: Mutex::new(graph),
            ml: ml::MLPipeline::new(),
            store: store::SecureStore::new(),
            news_cache: Mutex::new(None),
            feed_sync: Mutex::new(HashMap::new()),
            inbox_sync: Mutex::new(HashMap::new()),
        })
        .invoke_handler(tauri::generate_handler![
            get_feed,
            get_news,
            search_library,
            search_platform,
            store_credential,
            get_credentials,
            remove_credential,
            linkedin_connect,
            analyze_post,
            get_conversations,
            get_messages,
            monitor_profile,
            sync_all,
            get_stories,
            get_comments,
            sync_messages,
            send_message,
            mark_post_seen,
            rss_get_sources,
            rss_set_sources,
            rss_sync,
            get_rss_posts,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
