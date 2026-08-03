use no_pysop_lib::ingestion::twitter::TwitterIngester;
use no_pysop_lib::ingestion::PlatformIngester;
use no_pysop_lib::store::SecureStore;
use no_pysop_lib::types::{Credential, Platform};

fn env_credential() -> Option<Credential> {
    let cookie = std::env::var("TWITTER_COOKIE").ok()?;
    Some(Credential {
        platform: Platform::Twitter,
        session_token: cookie,
        user_id: std::env::var("TWITTER_USER_ID").unwrap_or_default(),
    })
}

fn stored_credential() -> Option<Credential> {
    SecureStore::new().get_credential(&Platform::Twitter).ok().flatten()
}

fn load_credential() -> Credential {
    env_credential()
        .or_else(stored_credential)
        .expect("set TWITTER_COOKIE env var or store a Twitter credential")
}

fn summarize_url(u: &str) -> String {
    format!("{}", u.split('/').nth(2).unwrap_or("?")).chars().take(48).collect::<String>()
}

#[tokio::test]
#[ignore = "requires TWITTER_COOKIE"]
async fn probe_twitter_session() {
    let cred = load_credential();
    let mut ing = TwitterIngester;
    match ing.refresh_session(&cred).await {
        Ok(_) => println!("TWITTER session: OK (token accepted)"),
        Err(e) => println!("TWITTER session: FAILED - {}", e),
    }
}

#[tokio::test]
#[ignore = "requires TWITTER_COOKIE"]
async fn probe_twitter_feed() {
    let cred = load_credential();
    let mut ing = TwitterIngester;
    match ing.fetch_feed(&cred).await {
        Ok(posts) => {
            println!("TWITTER FEED posts: {}", posts.len());
            for p in posts.iter().take(10) {
                println!(
                    "  id={} author={} video={} ts={} :: {}",
                    p.id,
                    p.author_username,
                    p.is_video,
                    p.timestamp,
                    p.content.chars().take(64).collect::<String>()
                );
            }
        }
        Err(e) => println!("TWITTER FEED ERROR: {}", e),
    }
}

#[tokio::test]
#[ignore = "requires TWITTER_COOKIE"]
async fn probe_twitter_inbox() {
    let cred = load_credential();
    let mut ing = TwitterIngester;
    match ing.fetch_inbox(&cred).await {
        Ok(threads) => {
            println!("TWITTER INBOX threads: {}", threads.len());
            for (conv, msgs) in threads.iter().take(5) {
                println!("  conv={} participants={:?} msgs={}", conv.id, conv.participants, msgs.len());
                for m in msgs.iter().take(3) {
                    let text = m.content.chars().take(40).collect::<String>();
                    println!("    {} ({}): {}", m.sender_id, m.timestamp, text);
                }
            }
        }
        Err(e) => println!("TWITTER INBOX ERROR: {}", e),
    }
}
