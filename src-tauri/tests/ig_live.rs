use no_pysop_lib::{ingestion, types};
use ingestion::PlatformIngester;

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
    let token = ingestion::instagram::ensure_sessionid_prefix(&raw);

    let cred = types::Credential {
        platform: types::Platform::Instagram,
        session_token: token,
        user_id: "0".into(),
    };

    let mut ing = ingestion::instagram::InstagramIngester;
    let posts = ing.fetch_feed(&cred).await.expect("fetch_feed failed");

    assert!(!posts.is_empty(), "expected ≥1 post from live feed");
    for p in &posts {
        assert!(!p.id.is_empty(), "id empty");
        assert!(!p.author_username.is_empty(), "username empty");
    }
}
