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

fn crate_ensure_session(token: &str) -> String {
    if token.trim().starts_with("sessionid=") || token.trim().starts_with("sessionid%3D") {
        token.to_string()
    } else {
        format!("sessionid={}", token)
    }
}
