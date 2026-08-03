use no_pysop_lib::ingestion::instagram::InstagramIngester;
use no_pysop_lib::ingestion::PlatformIngester;
use no_pysop_lib::types::Credential;

fn load_stored_credential() -> Credential {
    use base64::Engine;

    let path = dirs_next::data_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("no_pysop")
        .join("cred_Instagram.json");

    let encoded = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {}", path.display(), e));
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded.trim())
        .expect("base64 decode credential");
    serde_json::from_slice(&decoded).expect("parse credential")
}

fn summarize_url(u: &str) -> String {
    let host = u.split('/').nth(2).unwrap_or("?").to_string();
    format!("{} ({} chars)", host, u.len())
}

#[tokio::test]
#[ignore = "requires stored IG credential"]
async fn probe_feed() {
    let cred = load_stored_credential();
    let mut ing = InstagramIngester;
    let posts = ing.fetch_feed(&cred).await.expect("fetch_feed failed");
    println!("FEED posts: {}", posts.len());
    for p in posts.iter().take(5) {
        println!(
            "  id={} user={} video={} media=[{}]",
            p.id,
            p.author_username,
            p.is_video,
            p.media_urls.iter().map(|u| summarize_url(u)).collect::<Vec<_>>().join(", ")
        );
    }
    assert!(!posts.is_empty(), "expected >=1 post");
}

#[tokio::test]
#[ignore = "requires stored IG credential"]
async fn probe_stories() {
    let cred = load_stored_credential();
    let mut ing = InstagramIngester;
    let stories = ing.fetch_stories(&cred).await.expect("fetch_stories failed");
    println!("STORIES users: {}", stories.len());
    let with_items = stories.iter().filter(|s| !s.items.is_empty()).count();
    println!("STORIES users with items: {}", with_items);
    for s in stories.iter().filter(|s| !s.items.is_empty()).take(5) {
        println!(
            "  user={} items={} mutual={} close={} media=[{}]",
            s.username,
            s.items.len(),
            s.is_mutual,
            s.is_close_friend,
            s.items.first().map(|i| summarize_url(&i.media_url)).unwrap_or_default()
        );
    }
    assert!(with_items > 0, "expected stories with items");
}

#[tokio::test]
#[ignore = "requires stored IG credential"]
async fn probe_comments() {
    let cred = load_stored_credential();
    let mut ing = InstagramIngester;
    let posts = ing.fetch_feed(&cred).await.expect("fetch_feed failed");
    assert!(!posts.is_empty());
    let first = posts.first().unwrap();
    println!(
        "POST id={} author={} is_video={} media=[{}]",
        first.id,
        first.author_username,
        first.is_video,
        first.media_urls.first().map(|u| summarize_url(u)).unwrap_or_default()
    );
    let comments = ing.fetch_comments(&cred, &first.id).await.unwrap_or_default();
    println!("COMMENTS for {}: {}", first.id, comments.len());
    for c in comments.iter().take(5) {
        println!("  {}: {} (likes {})", c.author_username, c.content.chars().take(60).collect::<String>(), c.likes);
    }
}

#[tokio::test]
#[ignore = "requires stored IG credential"]
async fn probe_inbox() {
    let cred = load_stored_credential();
    let mut ing = InstagramIngester;
    match ing.fetch_inbox(&cred).await {
        Ok(threads) => {
            println!("THREADS: {}", threads.len());
            for (conv, msgs) in threads.iter().take(5) {
                println!(
                    "  conv={} participants={:?} msgs={} last={} unread={}",
                    conv.id.chars().take(12).collect::<String>(),
                    conv.participants,
                    msgs.len(),
                    conv.last_message_at,
                    conv.unread
                );
                for m in msgs.iter().take(3) {
                    println!("    {} ({}): {}", m.sender_id, m.timestamp, m.content.chars().take(50).collect::<String>());
                }
            }
        }
        Err(e) => println!("INBOX ERROR: {}", e),
    }
}

#[tokio::test]
#[ignore = "requires stored IG credential"]
async fn probe_messages() {
    let cred = load_stored_credential();
    let mut ing = InstagramIngester;
    match ing.fetch_messages(&cred).await {
        Ok(msgs) => {
            println!("MESSAGES: {}", msgs.len());
            for m in msgs.iter().take(5) {
                println!(
                    "  conv={} sender={} ts={} text={}",
                    m.conversation_id,
                    m.sender_id,
                    m.timestamp,
                    m.content.chars().take(40).collect::<String>()
                );
            }
        }
        Err(e) => println!("MESSAGES ERROR: {}", e),
    }
}

#[tokio::test]
#[ignore = "requires stored IG credential"]
async fn probe_media_via_proxy_headers() {
    let cred = load_stored_credential();
    let mut ing = InstagramIngester;
    let posts = ing.fetch_feed(&cred).await.expect("fetch_feed failed");

    let url = posts.iter().find(|p| !p.media_urls.is_empty()).map(|p| p.media_urls[0].clone());
    match url {
        Some(u) => {
            let token = crate_ensure_session(&cred.session_token);
            let client = no_pysop_lib::http::HttpClient::with_session(&token);
            let client = client.client().clone();
            let resp = client.get(&u).header("Range", "bytes=0-2047").send().await.expect("media get");
            let status = resp.status();
            let content_type = resp.headers().get("content-type").cloned();
            let bytes = resp.bytes().await.expect("read media body");
            println!(
                "MEDIA status={} content_type={:?} bytes_received={}",
                status.as_u16(),
                content_type,
                bytes.len()
            );
            assert!(status.is_success() || status.as_u16() == 206, "media fetch failed: {}", status);
        }
        None => println!("no media urls found"),
    }
}

#[tokio::test]
#[ignore = "requires stored IG credential"]
async fn probe_search() {
    let cred = load_stored_credential();
    let mut ing = InstagramIngester;
    let posts = ing.search_posts(&cred, "halal food trucks dallas").await;
    match posts {
        Ok(posts) => {
            println!("SEARCH posts: {}", posts.len());
            for p in posts.iter().take(8) {
                println!(
                    "  id={} user={} video={} media=[{}] caption={}",
                    p.id,
                    p.author_username,
                    p.is_video,
                    p.media_urls.iter().map(|u| summarize_url(u)).collect::<Vec<_>>().join(", "),
                    p.content.chars().take(50).collect::<String>()
                );
            }
            assert!(!posts.is_empty(), "expected search results");
        }
        Err(e) => println!("SEARCH ERROR: {}", e),
    }
}

#[tokio::test]
#[ignore = "requires stored IG credential"]
async fn probe_search_raw_endpoints() {
    use no_pysop_lib::http::HttpClient;

    let cred = load_stored_credential();
    let token = crate_ensure_session(&cred.session_token);
    let client = HttpClient::with_session(&token);
    let client = client.client().clone();

    let q = "halal%20food%20trucks%20dallas";
    let url = format!("https://www.instagram.com/web/search/topsearch/?query={}", q);
    let resp = client
        .get(&url)
        .header("X-IG-App-ID", "936619743392459")
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Referer", "https://www.instagram.com/explore/search/")
        .send()
        .await;
    match resp {
        Ok(r) => {
            let status = r.status().as_u16();
            let text = r.text().await.unwrap_or_default();
            println!("TOPSEARCH status={} len={} preview={}", status, text.len(), text.chars().take(300).collect::<String>());
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                let users = v["users"].as_array().map(|a| a.len()).unwrap_or(0);
                let tags = v["hashtags"].as_array().map(|a| a.len()).unwrap_or(0);
                let places = v["places"].as_array().map(|a| a.len()).unwrap_or(0);
                println!("TOPSEARCH users={} hashtags={} places={}", users, tags, places);
                for t in v["hashtags"].as_array().unwrap_or(&vec![]) {
                    if let Some(n) = t["hashtag"]["name"].as_str() {
                        println!("  TAG: #{}", n);
                    }
                }
                for u in v["users"].as_array().unwrap_or(&vec![]) {
                    if let Some(n) = u["user"]["username"].as_str() {
                        println!("  USER: @{}", n);
                    }
                }
            }
        }
        Err(e) => println!("TOPSEARCH ERROR: {}", e),
    }

    // Now fetch the first user's media directly
    if let Ok(r) = client
        .get("https://www.instagram.com/api/v1/users/66312721094/media/?count=5")
        .header("X-IG-App-ID", "936619743392459")
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Referer", "https://www.instagram.com/bobokamol_/")
        .send()
        .await
    {
        let status = r.status().as_u16();
        let text = r.text().await.unwrap_or_default();
        println!("USERMEDIA status={} len={} preview={}", status, text.len(), text.chars().take(300).collect::<String>());
    } else {
        println!("USERMEDIA request failed");
    }

    // GraphQL user timeline
    let vars = "%7B%22id%22%3A%2266312721094%22%2C%22first%22%3A12%7D";
    if let Ok(r) = client
        .get(&format!(
            "https://www.instagram.com/graphql/query/?query_hash=d04b0a864b4b54837c0d870b0e77f076&variables={}",
            vars
        ))
        .header("X-IG-App-ID", "936619743392459")
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Referer", "https://www.instagram.com/bobokamol_/")
        .send()
        .await
    {
        let status = r.status().as_u16();
        let text = r.text().await.unwrap_or_default();
        println!("GRAPHQL user timeline status={} len={} preview={}", status, text.len(), text.chars().take(400).collect::<String>());
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            let edges = v["data"]["user"]["edge_owner_to_timeline_media"]["edges"].as_array().map(|a| a.len()).unwrap_or(0);
            println!("GRAPHQL edges: {}", edges);
        }
    } else {
        println!("GRAPHQL user timeline request failed");
    }

    // hashtag sections
    if let Ok(r) = client
        .post("https://www.instagram.com/api/v1/tags/halalfoodtrucks/sections/")
        .json(&serde_json::json!({"surface":"grid","tab":"recent","page_type":"tags","include_persistent":true}))
        .header("X-IG-App-ID", "936619743392459")
        .header("X-Requested-With", "XMLHttpRequest")
        .header("Content-Type", "application/json")
        .header("Referer", "https://www.instagram.com/explore/tags/halalfoodtrucks/")
        .send()
        .await
    {
        let status = r.status().as_u16();
        let text = r.text().await.unwrap_or_default();
        println!("TAGS sections status={} len={} preview={}", status, text.len(), text.chars().take(400).collect::<String>());
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
            let secs = v["sections"].as_array().map(|a| a.len()).unwrap_or(0);
            println!("TAGS sections: {}", secs);
        }
    } else {
        println!("TAGS sections request failed");
    }

    // alternate user feed endpoints
    for (name, path) in [
        ("feed/user", "/api/v1/feed/user/66312721094/?count=5"),
        ("users/feed", "/api/v1/users/66312721094/feed/?count=5"),
    ] {
        if let Ok(r) = client
            .get(&format!("https://www.instagram.com{}", path))
            .header("X-IG-App-ID", "936619743392459")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", "https://www.instagram.com/bobokamol_/")
            .send()
            .await
        {
            let status = r.status().as_u16();
            let text = r.text().await.unwrap_or_default();
            println!("{} status={} len={} preview={}", name, status, text.len(), text.chars().take(200).collect::<String>());
        }
    }
}

fn crate_ensure_session(token: &str) -> String {
    if token.trim().starts_with("sessionid=") || token.trim().starts_with("sessionid%3D") {
        token.to_string()
    } else {
        format!("sessionid={}", token)
    }
}

#[tokio::test]
#[ignore = "requires stored IG credential"]
async fn probe_media_proxy_range_behavior() {
    use no_pysop_lib::media;
    use no_pysop_lib::store::SecureStore;
    use no_pysop_lib::types::Platform;

    let mut ing = InstagramIngester;
    let cred = load_stored_credential();
    let posts = ing.fetch_feed(&cred).await.expect("fetch_feed failed");
    let url = posts
        .iter()
        .find(|p| p.is_video && !p.media_urls.is_empty())
        .map(|p| p.media_urls[0].clone())
        .expect("no reel in feed");

    let store = SecureStore::new();
    assert!(store.has_credential(&Platform::Instagram), "no stored cred");

    let empty_headers = tauri::http::HeaderMap::new();
    let mut range_headers = tauri::http::HeaderMap::new();
    range_headers.insert("range", "bytes=0-".parse().unwrap());

    // proxy uses reqwest::blocking; run it off the tokio runtime
    let handle = std::thread::spawn(move || {
        let no_range = media::proxy(&store, &url, &empty_headers);
        match &no_range {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let cl = resp.headers().get("content-length").cloned();
                let ct = resp.headers().get("content-type").cloned();
                let len = resp.body().len();
                println!(
                    "PROXY no-range: status={} content_type={:?} content_length={:?} body_bytes={}",
                    status, ct, cl, len
                );
            }
            Err(e) => println!("PROXY no-range ERROR: {}", e),
        }

        let open_range = media::proxy(&store, &url, &range_headers);
        match &open_range {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let cl = resp.headers().get("content-length").cloned();
                let cr = resp.headers().get("content-range").cloned();
                let ct = resp.headers().get("content-type").cloned();
                let len = resp.body().len();
                println!(
                    "PROXY range=bytes=0-: status={} content_type={:?} content_range={:?} content_length={:?} body_bytes={}",
                    status, ct, cr, cl, len
                );
            }
            Err(e) => println!("PROXY range=bytes=0- ERROR: {}", e),
        }
    });
    handle.join().expect("proxy thread panicked");
}
