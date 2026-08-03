use no_pysop_lib::ingestion::linkedin::LinkedInIngester;
use no_pysop_lib::ingestion::PlatformIngester;
use no_pysop_lib::store::SecureStore;
use no_pysop_lib::types::{Credential, Platform};

fn env_credential() -> Option<Credential> {
    let cookie = std::env::var("LINKEDIN_COOKIE").ok()?;
    Some(Credential {
        platform: Platform::LinkedIn,
        session_token: cookie,
        user_id: std::env::var("LINKEDIN_USER_ID").unwrap_or_default(),
    })
}

fn stored_credential() -> Option<Credential> {
    SecureStore::new().get_credential(&Platform::LinkedIn).ok().flatten()
}

fn load_credential() -> Credential {
    env_credential()
        .or_else(stored_credential)
        .expect("set LINKEDIN_COOKIE env var or store a LinkedIn credential")
}

fn summarize_url(u: &str) -> String {
    format!("{}", u.split('/').nth(2).unwrap_or("?")).chars().take(48).collect::<String>()
}

#[tokio::test]
#[ignore = "requires LINKEDIN_COOKIE"]
async fn probe_linkedin_session() {
    let cred = load_credential();
    let mut ing = LinkedInIngester;
    match ing.refresh_session(&cred).await {
        Ok(_) => println!("LINKEDIN session: OK (token accepted)"),
        Err(e) => println!("LINKEDIN session: FAILED - {}", e),
    }
}

#[tokio::test]
#[ignore = "requires LINKEDIN_COOKIE"]
async fn probe_linkedin_feed() {
    let cred = load_credential();
    let mut ing = LinkedInIngester;
    match ing.fetch_feed(&cred).await {
        Ok(posts) => {
            println!("LINKEDIN FEED posts: {}", posts.len());
            for p in posts.iter().take(10) {
                println!(
                    "  id={} author={} video={} media=[{}] ts={} :: {}",
                    p.id,
                    p.author_username,
                    p.is_video,
                    p.timestamp,
                    p.media_urls.iter().map(|u| summarize_url(u)).collect::<Vec<_>>().join(", "),
                    p.content.chars().take(64).collect::<String>()
                );
            }
        }
        Err(e) => println!("LINKEDIN FEED ERROR: {}", e),
    }
}

#[tokio::test]
#[ignore = "requires LINKEDIN_COOKIE"]
async fn probe_linkedin_inbox() {
    let cred = load_credential();
    let mut ing = LinkedInIngester;
    match ing.fetch_inbox(&cred).await {
        Ok(threads) => {
            println!("LINKEDIN INBOX threads: {}", threads.len());
            for (conv, msgs) in threads.iter().take(5) {
                println!("  conv={} participants={:?} msgs={}", conv.id, conv.participants, msgs.len());
                for m in msgs.iter().take(3) {
                    let text = m.content.chars().take(40).collect::<String>();
                    println!("    {} ({}): {}", m.sender_id, m.timestamp, text);
                }
            }
        }
        Err(e) => println!("LINKEDIN INBOX ERROR: {}", e),
    }
}

#[tokio::test]
#[ignore = "requires LINKEDIN_COOKIE"]
async fn probe_linkedin_profile() {
    let cred = load_credential();
    let usernames = std::env::var("LINKEDIN_PROFILE_USERNAME").unwrap_or_default();
    if usernames.is_empty() {
        println!("LINKEDIN PROFILE: set LINKEDIN_PROFILE_USERNAME to probe");
        return;
    }
    let mut ing = LinkedInIngester;
    match ing.fetch_profile(&cred, &usernames).await {
        Ok(user) => println!("LINKEDIN PROFILE {}: id={} followers={} following={}", usernames, user.id, user.followers.len(), user.follows.len()),
        Err(e) => println!("LINKEDIN PROFILE ERROR: {}", e),
    }
}