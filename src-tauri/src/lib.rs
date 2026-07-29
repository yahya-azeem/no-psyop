#![allow(dead_code)]

pub mod types;
mod store;
mod graph;
pub mod http;
pub mod ingestion;
mod ml;
mod bridge;
mod search;

use std::sync::Mutex;
use tauri::State;
use ingestion::PlatformIngester;

struct AppState {
    graph: Mutex<graph::GraphEngine>,
    ml: ml::MLPipeline,
    search: Mutex<search::SemanticSearch>,
    bridge: Mutex<bridge::UnifiedBridge>,
    store: store::SecureStore,
}

#[tauri::command]
fn get_feed(state: State<AppState>, user_id: Option<String>, platform: Option<String>) -> Result<Vec<types::FeedItem>, String> {
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

    let posts = match platform {
        "Instagram" => graph.get_feed(&types::Platform::Instagram, 20).map(|v| v.into_iter().map(wrap).collect()),
        "Twitter" => graph.get_feed(&types::Platform::Twitter, 20).map(|v| v.into_iter().map(wrap).collect()),
        "LinkedIn" => graph.get_feed(&types::Platform::LinkedIn, 20).map(|v| v.into_iter().map(wrap).collect()),
        _ => {
            let mut all: Vec<types::FeedItem> = Vec::new();
            for p in [types::Platform::Instagram, types::Platform::Twitter, types::Platform::LinkedIn] {
                if let Ok(v) = graph.get_feed(&p, 20) {
                    all.extend(v.into_iter().map(|p| wrap(p)));
                }
            }
            all.sort_by(|a, b| b.post.timestamp.cmp(&a.post.timestamp));
            Ok(all)
        }
    }?;
    Ok(posts)
}

#[tauri::command]
fn search_posts(state: State<AppState>, query: String, platform: Option<String>) -> Result<Vec<String>, String> {
    let search = state.search.lock().map_err(|e| e.to_string())?;
    let platform = platform.as_ref().map(|s| match s.as_str() {
        "Instagram" => types::Platform::Instagram,
        "Twitter" => types::Platform::Twitter,
        "LinkedIn" => types::Platform::LinkedIn,
        _ => types::Platform::Twitter,
    });
    Ok(search.search_text(&query, platform.as_ref(), 20))
}

#[tauri::command]
fn store_credential(state: State<AppState>, platform: String, session_token: String, user_id: String) -> Result<(), String> {
    let p = match platform.as_str() {
        "Instagram" => types::Platform::Instagram,
        "Twitter" => types::Platform::Twitter,
        "LinkedIn" => types::Platform::LinkedIn,
        _ => return Err("unknown platform".into()),
    };
    let cred = types::Credential { platform: p, session_token, user_id };
    state.store.store_credential(&cred)
}

#[tauri::command]
fn get_credentials(state: State<AppState>) -> Result<Vec<types::Credential>, String> {
    let store = &state.store;
    let mut creds = Vec::new();
    for p in &[types::Platform::Instagram, types::Platform::Twitter, types::Platform::LinkedIn] {
        if let Ok(Some(cred)) = store.get_credential(p) {
            creds.push(cred);
        }
    }
    Ok(creds)
}

#[tauri::command]
fn remove_credential(state: State<AppState>, platform: String) -> Result<(), String> {
    let p = match platform.as_str() {
        "Instagram" => types::Platform::Instagram,
        "Twitter" => types::Platform::Twitter,
        "LinkedIn" => types::Platform::LinkedIn,
        _ => return Err("unknown platform".into()),
    };
    state.store.remove_credential(&p)
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
        liker_ids: vec![],
        commenter_ids: vec![],
        timestamp: 0,
        is_video: false,
        engagement_score: None,
        is_synthetic: None,
        vector_embedding: None,
    };
    Ok(state.ml.filter_post(&post))
}

#[tauri::command]
fn get_conversations(state: State<AppState>, platform: Option<String>) -> Result<Vec<types::Conversation>, String> {
    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    match platform {
        Some(p) => {
            let p = match p.as_str() {
                "Instagram" => types::Platform::Instagram,
                "Twitter" => types::Platform::Twitter,
                "LinkedIn" => types::Platform::LinkedIn,
                _ => return Err("unknown platform".into()),
            };
            graph.get_conversations(&p)
        }
        None => graph.get_all_conversations(),
    }
}

#[tauri::command]
fn get_messages(state: State<AppState>, conversation_id: String, platform: String) -> Result<Vec<types::Message>, String> {
    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    let p = match platform.as_str() {
        "Instagram" => types::Platform::Instagram,
        "Twitter" => types::Platform::Twitter,
        "LinkedIn" => types::Platform::LinkedIn,
        _ => return Err("unknown platform".into()),
    };
    graph.get_messages(&conversation_id, &p)
}

#[tauri::command]
async fn search_instagram(state: State<'_, AppState>, query: String) -> Result<Vec<types::Post>, String> {
    let cred = state.store.get_credential(&types::Platform::Instagram)?
        .ok_or_else(|| "Instagram not connected".to_string())?;
    let mut ing = ingestion::instagram::InstagramIngester;
    ing.search_posts(&cred, &query).await
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
    };

    let graph = state.graph.lock().map_err(|e| e.to_string())?;
    graph.sync_user(profile.clone())?;
    Ok(profile)
}

#[tauri::command]
async fn sync_all(state: State<'_, AppState>) -> Result<types::SyncResult, String> {
    let creds = {
        let store = &state.store;
        let mut all = Vec::new();
        for p in &[types::Platform::Instagram, types::Platform::Twitter, types::Platform::LinkedIn] {
            if let Ok(Some(cred)) = store.get_credential(p) {
                all.push(cred);
            }
        }
        all
    };

    if creds.is_empty() {
        return Err("No credentials configured. Connect a platform in Settings first.".into());
    }

    let mut engine = ingestion::IngestionEngine::new();
    let results = engine.fetch_all_feeds(&creds).await;

    let mut posts_added = 0usize;
    let mut errors = Vec::new();

    for (platform, result) in &results {
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
                if let Ok(mut search) = state.search.lock() {
                    for post in posts {
                        search.index_post(post);
                    }
                }
            }
            Err(e) => {
                errors.push(format!("{:?}: {}", platform, e));
            }
        }
    }

    Ok(types::SyncResult {
        posts_added,
        messages_added: 0,
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

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            graph: Mutex::new(graph),
            ml: ml::MLPipeline::new(),
            search: Mutex::new(search::SemanticSearch::new()),
            bridge: Mutex::new(bridge::UnifiedBridge::new()),
            store: store::SecureStore::new(),
        })
        .invoke_handler(tauri::generate_handler![
            get_feed,
            search_posts,
            store_credential,
            get_credentials,
            remove_credential,
            analyze_post,
            get_conversations,
            get_messages,
            monitor_profile,
            sync_all,
            search_instagram,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
