use no_pysop_lib::{ingestion, types};
use ingestion::PlatformIngester;

fn make_cred(raw: &str) -> types::Credential {
    let token = ingestion::instagram::ensure_sessionid_prefix(raw);
    types::Credential {
        platform: types::Platform::Instagram,
        session_token: token,
        user_id: "0".into(),
    }
}

#[test]
fn test_token_prefix() {
    assert_eq!(
        ingestion::instagram::ensure_sessionid_prefix("abc123"),
        "sessionid=abc123"
    );
    assert_eq!(
        ingestion::instagram::ensure_sessionid_prefix("sessionid=abc123"),
        "sessionid=abc123"
    );
}

#[tokio::test]
#[ignore = "requires IG_SESSION_TOKEN env var and live Instagram API"]
async fn test_fetch_feed_live() {
    let raw = std::env::var("IG_SESSION_TOKEN").expect("set IG_SESSION_TOKEN env var");
    let cred = make_cred(&raw);
    let mut ing = ingestion::instagram::InstagramIngester;
    let posts = ing.fetch_feed(&cred).await.expect("fetch_feed failed");

    assert!(!posts.is_empty(), "expected ≥1 post from live feed");
    for p in &posts {
        assert!(!p.id.is_empty(), "id empty");
        assert!(!p.author_username.is_empty(), "username empty");
    }
}

#[tokio::test]
#[ignore = "requires IG_SESSION_TOKEN env var and live Instagram API"]
async fn test_fetch_messages_live() {
    let raw = std::env::var("IG_SESSION_TOKEN").expect("set IG_SESSION_TOKEN env var");
    let cred = make_cred(&raw);
    let mut ing = ingestion::instagram::InstagramIngester;
    let msgs = ing.fetch_messages(&cred).await.expect("fetch_messages failed");
    // DMs may be empty if user has no conversations — just check it doesn't error
    for m in &msgs {
        assert!(!m.id.is_empty(), "message id empty");
        assert!(m.platform == types::Platform::Instagram, "wrong platform");
    }
}

#[tokio::test]
#[ignore = "requires IG_SESSION_TOKEN env var and live Instagram API"]
async fn test_search_instagram_live() {
    let raw = std::env::var("IG_SESSION_TOKEN").expect("set IG_SESSION_TOKEN env var");
    let cred = make_cred(&raw);
    let mut ing = ingestion::instagram::InstagramIngester;
    let posts = ing.search_posts(&cred, "halal food trucks dallas").await.expect("search_posts failed");
    // Search may return 0 results — just check no errors
    for p in &posts {
        assert!(!p.id.is_empty(), "id empty");
    }
}

#[tokio::test]
#[ignore = "requires IG_SESSION_TOKEN env var and live Instagram API"]
async fn test_fetch_stories_live() {
    let raw = std::env::var("IG_SESSION_TOKEN").expect("set IG_SESSION_TOKEN env var");
    let cred = make_cred(&raw);
    let mut ing = ingestion::instagram::InstagramIngester;
    let stories = ing.fetch_stories(&cred).await.expect("fetch_stories failed");
    // Stories tray may be empty if no followed accounts posted recently — check shape only
    for s in &stories {
        assert!(!s.username.is_empty(), "story username empty");
        for item in &s.items {
            assert!(!item.media_url.is_empty(), "story media url empty");
        }
    }
}
