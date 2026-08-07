use async_trait::async_trait;

use crate::http::HttpClient;
use crate::http::xproxy::XProxy;
use crate::types::{Credential, Message, Platform, Post, SocialUser};
use super::PlatformIngester;

const API_BASE: &str = "https://www.linkedin.com/voyager/api";

// Feed query hashes observed in the web client; the client rotates them, so we
// discover a fresh one at runtime and fall back to these when unavailable.
const KNOWN_FEED_QUERY_HASHES: [&str; 2] = [
    "7a50ef8ba5a7865c23ad5df46f735709",
    "923020905727c01516495a0ac90bb475",
];

fn normalize_ts(raw: u64) -> u64 {
    if raw > 1_000_000_000_000 {
        raw / 1_000
    } else {
        raw
    }
}

fn feed_query_id_from_text(text: &str) -> Option<String> {
    let re = regex::Regex::new(r"voyagerFeedDashMainFeed\.([0-9a-f]{16,})").ok()?;
    re.captures(text)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str().to_string())
}

pub struct LinkedInIngester;

impl LinkedInIngester {
    fn csrf_token(&self, credential: &Credential) -> String {
        credential.session_token
            .split(';')
            .find(|p| p.trim().starts_with("JSESSIONID="))
            .and_then(|p| p.split('=').nth(1))
            .map(|s| s.trim().trim_matches('"').to_string())
            .unwrap_or_default()
    }

    #[allow(dead_code)] // used by unit tests; requests authenticate via the cookie jar
    fn auth_header(&self, credential: &Credential) -> String {
        let token = credential.session_token
            .split(';')
            .find(|p| p.trim().starts_with("li_at="))
            .and_then(|p| p.split('=').nth(1))
            .unwrap_or("");
        format!("Bearer {}", token)
    }

    fn build_client(&self, credential: &Credential) -> HttpClient {
        // session_token already carries li_at + JSESSIONID; with_session seeds them all.
        HttpClient::with_session(&credential.session_token)
    }

    fn parse_feed_items(&self, body: &serde_json::Value) -> Vec<Post> {
        let mut posts = Vec::new();

        if let Some(elements) = body["data"]["feedDashUrs"]["elements"].as_array() {
            for element in elements {
                let activity = &element["activity"];
                if !activity.is_null() {
                    if let Some(post) = self.parse_activity(activity) {
                        posts.push(post);
                    }
                }
            }
        }

        if let Some(included) = body["included"].as_array() {
            for item in included {
                let activity = &item["activity"];
                if !activity.is_null() {
                    if let Some(post) = self.parse_activity(activity) {
                        if !posts.iter().any(|p| p.id == post.id) {
                            posts.push(post);
                        }
                    }
                }
            }
        }

        posts
    }

    fn parse_activity(&self, activity: &serde_json::Value) -> Option<Post> {
        let urn = activity["$urn"].as_str().or_else(|| activity["urn"].as_str())?;
        let id = urn.rsplit(':').next()?.to_string();

        let content = activity["summary"]["text"].as_str()
            .or_else(|| activity["commentary"]["text"].as_str())
            .or_else(|| activity["headline"]["text"].as_str())
            .unwrap_or("")
            .to_string();

        let author_urn = activity["actor"]["$urn"].as_str()
            .or_else(|| activity["actor"]["urn"].as_str())
            .unwrap_or("");

        let author_id = author_urn.rsplit(':').next().unwrap_or("").to_string();
        let author_username = activity["actor"]["name"].as_str()
            .or_else(|| activity["actor"]["miniProfile"]["publicIdentifier"].as_str())
            .unwrap_or(&author_id)
            .to_string();

        let timestamp = normalize_ts(activity["temporal"]["time"].as_i64().unwrap_or(0) as u64);

        let mut media_urls = Vec::new();
        if let Some(images) = activity["images"].as_array() {
            for img in images {
                if let Some(url) = img["url"].as_str().or_else(|| {
                    img["attributes"].as_array()
                        .and_then(|attrs| attrs.first())
                        .and_then(|a| a["detailData"]["url"].as_str())
                }) {
                    media_urls.push(url.to_string());
                }
            }
        }

        let is_video = activity["content"]["type"].as_str() == Some("video")
            || activity["type"].as_str() == Some("video");

        if is_video {
            if let Some(playlists) = activity["content"]["playlists"].as_array() {
                if let Some(first) = playlists.first() {
                    if let Some(url) = first["url"].as_str() {
                        media_urls.push(url.to_string());
                    }
                }
            }
        }

        let liker_ids: Vec<String> = activity["likes"]["elements"].as_array()
            .map(|elements| {
                elements.iter()
                    .filter_map(|e| e["actor"]["$urn"].as_str())
                    .filter_map(|urn| urn.rsplit(':').next().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Some(Post {
            id,
            platform: Platform::LinkedIn,
            author_id,
            author_username,
            content,
            media_urls,
            poster_url: None,
            liker_ids,
            commenter_ids: Vec::new(),
            timestamp,
            is_video,
            author_is_mutual: None,
            author_is_close_friend: None,
            engagement_score: None,
            is_synthetic: None,
            vector_embedding: None,
        })
    }

    fn parse_graphql_feed(&self, body: &serde_json::Value) -> Vec<Post> {
        let mut posts: Vec<Post> = Vec::new();
        let feed = &body["data"]["data"]["feedDashMainFeedByMainFeed"];
        let feed = if feed.is_null() {
            &body["data"]["data"]["feedDashMainFeed"]
        } else {
            feed
        };

        // element references are urns (strings) in the GraphQL response
        let mut refs: Vec<String> = Vec::new();
        if let Some(els) = feed["*elements"].as_array() {
            for e in els {
                if let Some(a) = e.get("activity") {
                    if let Some(p) = self.parse_activity(a) {
                        if !posts.iter().any(|p2| p2.id == p.id) {
                            posts.push(p);
                        }
                    }
                }
                if let Some(s) = e.as_str() {
                    refs.push(s.to_string());
                } else if let Some(u) = e["entityUrn"].as_str().or_else(|| e["$urn"].as_str()) {
                    refs.push(u.to_string());
                }
            }
        }

        let included = body["included"]
            .as_array()
            .or_else(|| body["data"]["included"].as_array())
            .map(|a| a.as_slice())
            .unwrap_or(&[]);

        for item in included {
            let urn = item["entityUrn"].as_str()
                .or_else(|| item["$urn"].as_str())
                .or_else(|| item["urn"].as_str())
                .unwrap_or("");
            if !refs.iter().any(|r| r == urn) {
                continue;
            }
            if let Some(post) = self.parse_update(item) {
                if !posts.iter().any(|p| p.id == post.id) {
                    posts.push(post);
                }
            }
        }
        posts
    }

    fn parse_update(&self, item: &serde_json::Value) -> Option<Post> {
        let update = item.get("updateV2").filter(|v| !v.is_null()).unwrap_or(item);
        let urn = item["$urn"].as_str()
            .or_else(|| item["entityUrn"].as_str())
            .or_else(|| item["urn"].as_str())?;
        let id = urn.rsplit(':').next()?.to_string();

        let content = update["commentary"]["commentary"]["text"].as_str()
            .or_else(|| update["commentary"]["text"].as_str())
            .or_else(|| update["summary"]["text"].as_str())
            .or_else(|| update["headline"]["text"].as_str())
            .unwrap_or("")
            .to_string();

        let actor_urn = update["actor"].as_str()
            .or_else(|| update["actor"]["$urn"].as_str())
            .or_else(|| update["actor"]["urn"].as_str())
            .unwrap_or("");
        let author_id = actor_urn.rsplit(':').next().unwrap_or("").to_string();
        let author_username = update["actorName"].as_str()
            .or_else(|| update["actor"]["name"].as_str())
            .or_else(|| update["actor"]["miniProfile"]["publicIdentifier"].as_str())
            .unwrap_or(&author_id)
            .to_string();

        let timestamp = normalize_ts(update["*metadata"]["time"].as_i64().unwrap_or(0) as u64);

        let mut media_urls = Vec::new();
        let is_video = update["content"]["video"].is_object()
            || update["content"]["type"].as_str() == Some("video")
            || update["type"].as_str() == Some("video");
        if let Some(imgs) = update["content"]["images"].as_array() {
            for i in imgs {
                if let Some(u) = i["url"].as_str() {
                    media_urls.push(u.to_string());
                }
            }
        }
        if is_video {
            if let Some(pl) = update["content"]["playlists"].as_array() {
                if let Some(f) = pl.first() {
                    if let Some(u) = f["url"].as_str() {
                        media_urls.push(u.to_string());
                    }
                }
            }
        }

        let liker_ids: Vec<String> = update["likes"]["elements"].as_array()
            .map(|els| {
                els.iter()
                    .filter_map(|e| e["actor"].as_str())
                    .filter_map(|u| u.rsplit(':').next().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        Some(Post {
            id,
            platform: Platform::LinkedIn,
            author_id,
            author_username,
            content,
            media_urls,
            poster_url: None,
            liker_ids,
            commenter_ids: Vec::new(),
            timestamp,
            is_video,
            author_is_mutual: None,
            author_is_close_friend: None,
            engagement_score: None,
            is_synthetic: None,
            vector_embedding: None,
        })
    }

    async fn discover_feed_query_id(&self, client: &HttpClient, csrf: &str) -> Option<String> {
        let page_url = "https://www.linkedin.com/feed/";
        let resp = client.client()
            .get(page_url)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .header("Csrf-Token", csrf)
            .header("X-RestLi-Protocol-Version", "2.0.0")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", page_url)
            .send()
            .await
            .ok()?;
        let html = resp.text().await.ok()?;
        if let Some(qid) = feed_query_id_from_text(&html) {
            return Some(qid);
        }

        // fall back to scanning bundle URLs referenced by the page
        let script_re = regex::Regex::new(r#"<script[^>]+src="([^"]+)""#).ok()?;
        for cap in script_re.captures_iter(&html).take(4) {
            let src = cap.get(1)?.as_str();
            let url = if src.starts_with('/') {
                format!("https://www.linkedin.com{}", src)
            } else if src.starts_with("http") {
                src.to_string()
            } else {
                continue;
            };
            let resp = client.client()
                .get(url)
                .header("Referer", "https://www.linkedin.com/feed/")
                .send()
                .await
                .ok()?;
            let js = resp.text().await.ok()?;
            if let Some(qid) = feed_query_id_from_text(&js) {
                return Some(qid);
            }
        }
        None
    }

    async fn fetch_graphql_feed(&self, client: &HttpClient, csrf: &str) -> Option<Vec<Post>> {
        let discovered = self.discover_feed_query_id(client, csrf).await;
        let mut hashes: Vec<String> = Vec::new();
        if let Some(q) = discovered {
            hashes.push(q);
        }
        for h in &KNOWN_FEED_QUERY_HASHES {
            hashes.push(h.to_string());
        }

        for hash in hashes {
            let url = format!(
                "{}/graphql?queryId=voyagerFeedDashMainFeed.{}&variables={}&csrfToken={}",
                API_BASE,
                hash,
                urlencoding::encode("(start:0,count:10,sortOrder:MEMBER_SETTING)"),
                urlencoding::encode(csrf)
            );
            if let Ok(body) = self.get_voyager(client, &url, csrf, "https://www.linkedin.com/feed/").await {
                let posts = self.parse_graphql_feed(&body);
                if !posts.is_empty() {
                    return Some(posts);
                }
            }
        }
        None
    }

    fn extract_profile(&self, body: &serde_json::Value) -> SocialUser {
        let id = body["data"]["user"]["urn"]
            .as_str()
            .and_then(|u| u.rsplit(':').next())
            .unwrap_or("")
            .to_string();

        let username = body["data"]["user"]["publicIdentifier"]
            .as_str()
            .unwrap_or("")
            .to_string();

        let followers = body["data"]["user"]["followers"]["elements"]
            .as_array()
            .map(|elements| {
                elements.iter()
                    .filter_map(|e| e["urn"].as_str())
                    .filter_map(|u| u.rsplit(':').next().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        let following = body["data"]["user"]["following"]["elements"]
            .as_array()
            .map(|elements| {
                elements.iter()
                    .filter_map(|e| e["urn"].as_str())
                    .filter_map(|u| u.rsplit(':').next().map(String::from))
                    .collect()
            })
            .unwrap_or_default();

        SocialUser {
            id,
            platform: Platform::LinkedIn,
            username,
            follows: following,
            followers,
            last_sync: chrono::Utc::now().timestamp() as u64,
        }
    }

    fn extract_messages(&self, body: &serde_json::Value) -> Vec<Message> {
        let mut msgs = Vec::new();

        if let Some(elements) = body["data"]["conversations"]["elements"].as_array() {
            for conv in elements {
                let conv_id = conv["$urn"].as_str()
                    .or_else(|| conv["entityUrn"].as_str())
                    .unwrap_or("")
                    .to_string();

                if let Some(events) = conv["events"].as_array() {
                    for event in events {
                        let msg_id = event["$id"].as_str().unwrap_or("").to_string();
                        let text = event["content"]["text"].as_str()
                            .or_else(|| event["body"].as_str())
                            .unwrap_or("")
                            .to_string();
                        let sender = event["from"]["urn"].as_str()
                            .and_then(|u| u.rsplit(':').next())
                            .unwrap_or("")
                            .to_string();
                        let ts = normalize_ts((event["createdAt"].as_i64().unwrap_or(0)) as u64);

                        msgs.push(Message {
                            id: msg_id,
                            platform: Platform::LinkedIn,
                            conversation_id: conv_id.clone(),
                            sender_id: sender,
                            content: text,
                            timestamp: ts,
                            is_mine: false,
                        });
                    }
                }
            }
        }

        msgs
    }
}

#[async_trait]
impl PlatformIngester for LinkedInIngester {
    fn platform(&self) -> Platform {
        Platform::LinkedIn
    }

    async fn fetch_feed(&mut self, credential: &Credential) -> Result<Vec<Post>, String> {
        // Primary: rotating GraphQL feed + legacy REST feed via plain HTTP.
        match self.http_feed(credential).await {
            Ok(posts) if !posts.is_empty() => return Ok(posts),
            _ => {}
        }

        // Fallback: LinkedIn rejects direct-HTTP feed calls (400/404), so scrape
        // the authenticated browser feed through the sidecar instead.
        let body = XProxy::linkedin_feed(&credential.session_token).await?;
        let posts = self.parse_browser_feed(&body);
        if posts.is_empty() {
            return Err("linkedin feed returned no posts (HTTP + browser both empty)".into());
        }
        Ok(posts)
    }

    async fn fetch_profile(&mut self, credential: &Credential, _username: &str) -> Result<SocialUser, String> {
        let client = self.build_client(credential);
        let csrf = self.csrf_token(credential);

        let url = format!(
            "{}/identity/profiles/{}/profileView?csrfToken={}",
            API_BASE, _username, urlencoding::encode(&csrf)
        );

        let followers_url = format!(
            "{}/identity/profiles/{}/connections?start=0&count=50&csrfToken={}",
            API_BASE, _username, urlencoding::encode(&csrf)
        );

        let body = self.get_voyager(&client, &url, &csrf, "https://www.linkedin.com/in/").await?;
        let mut profile = self.extract_profile(&body);

        if let Ok(followers_body) = self.get_voyager(&client, &followers_url, &csrf, "https://www.linkedin.com/in/").await {
            if let Some(elements) = followers_body["data"]["connections"]["elements"].as_array() {
                for element in elements {
                    if let Some(urn) = element["$urn"].as_str().or(element["urn"].as_str()) {
                        if let Some(id) = urn.rsplit(':').next() {
                            profile.followers.push(id.to_string());
                        }
                    }
                }
            }
        }

        Ok(profile)
    }

    async fn fetch_messages(&mut self, credential: &Credential) -> Result<Vec<Message>, String> {
        let client = self.build_client(credential);
        let csrf = self.csrf_token(credential);

        let url = format!(
            "{}/messaging/conversations?start=0&count=20&csrfToken={}",
            API_BASE,
            urlencoding::encode(&csrf)
        );

        // LinkedIn has been deprecating the Voyager REST messaging endpoint;
        // fall back to the device-trusted browser inbox when it errors/empties.
        match self.get_voyager(&client, &url, &csrf, "https://www.linkedin.com/messaging/").await {
            Ok(body) => {
                let msgs = self.extract_messages(&body);
                if !msgs.is_empty() {
                    return Ok(msgs);
                }
            }
            Err(_) => {}
        }

        let browser = XProxy::linkedin_messages().await?;
        Ok(self.parse_browser_messages(&browser))
    }

    async fn send_message(&mut self, credential: &Credential, conversation_id: &str, content: &str) -> Result<(), String> {
        // Sending goes through the device-trusted browser profile (LinkedIn
        // rejects Voyager messaging writes from generic HTTP clients).
        let _ = credential;
        XProxy::linkedin_send(conversation_id, content).await
    }

    async fn refresh_session(&mut self, credential: &Credential) -> Result<Credential, String> {
        let client = self.build_client(credential);
        let csrf = self.csrf_token(credential);

        let url = format!("{}/me?csrfToken={}", API_BASE, urlencoding::encode(&csrf));

        match self.get_voyager(&client, &url, &csrf, "https://www.linkedin.com/").await {
            Ok(_) => Ok(credential.clone()),
            Err(e) => Err(format!("linkedin session expired: {}", e)),
        }
    }
}

impl LinkedInIngester {
    /// Try the direct-HTTP feed paths (GraphQL discovery, then legacy REST).
    async fn http_feed(&self, credential: &Credential) -> Result<Vec<Post>, String> {
        let client = self.build_client(credential);
        let csrf = self.csrf_token(credential);

        // primary: rotating GraphQL feed (query id discovered from web client)
        if let Some(posts) = self.fetch_graphql_feed(&client, &csrf).await {
            return Ok(posts);
        }

        // fallback: legacy REST feed
        let url = format!(
            "{}/feed/dashUpdates?start=0&count=10&feedType=ALL&feedModuleType=HYPE_FEED&csrfToken={}",
            API_BASE,
            urlencoding::encode(&csrf)
        );
        let body = self.get_voyager(&client, &url, &csrf, "https://www.linkedin.com/feed/").await?;
        Ok(self.parse_feed_items(&body))
    }

    /// Map a browser-scraped inbox (`messengerConversationsBySyncToken.elements`)
    /// into `Message`s. Each conversation ships its latest message preview in
    /// the response, which we turn into one inbox row.
    fn parse_browser_messages(&self, body: &serde_json::Value) -> Vec<Message> {
        let mut msgs = Vec::new();
        let convs = body["data"]["messengerConversationsBySyncToken"]["elements"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        for conv in convs {
            let conv_id = conv["backendUrn"]
                .as_str()
                .or_else(|| conv["entityUrn"].as_str())
                .unwrap_or("")
                .to_string();
            let messages = conv["messages"]["elements"].as_array().cloned().unwrap_or_default();
            let mut conv_msgs: Vec<Message> = messages
                .into_iter()
                .map(|m| {
                    let id = m["backendUrn"]
                        .as_str()
                        .or_else(|| m["entityUrn"].as_str())
                        .unwrap_or("")
                        .to_string();
                    let content = m["body"]["text"]
                        .as_str()
                        .or_else(|| m["renderContentFallbackText"].as_str())
                        .unwrap_or("")
                        .to_string();
                    let sender = m["sender"]["entityUrn"].as_str()
                        .or_else(|| m["actor"].as_str())
                        .and_then(|u| u.rsplit(':').next())
                        .unwrap_or("")
                        .to_string();
                    let ts = normalize_ts(m["deliveredAt"].as_u64().unwrap_or(0));
                    Message {
                        id,
                        platform: Platform::LinkedIn,
                        conversation_id: conv_id.clone(),
                        sender_id: sender,
                        content: content,
                        timestamp: ts,
                        is_mine: false,
                    }
                })
                .collect();
            msgs.append(&mut conv_msgs);
        }
        msgs
    }

    /// Map the browser-scraped feed (`{"posts":[{...}]}`) into `Post`s.
    fn parse_browser_feed(&self, body: &serde_json::Value) -> Vec<Post> {
        let mut posts = Vec::new();
        let Some(items) = body["posts"].as_array() else {
            return posts;
        };
        for p in items {
            let id = p["id"].as_str().unwrap_or("").to_string();
            if id.is_empty() {
                continue;
            }
            let author_username = p["username"].as_str()
                .filter(|s| !s.is_empty())
                .or_else(|| p["author"].as_str())
                .unwrap_or("")
                .to_string();
            let author_id = p["author"].as_str().unwrap_or("").to_string();
            let content = p["text"].as_str().unwrap_or("").to_string();
            let is_video = p["is_video"].as_bool().unwrap_or(false);
            let is_connection = p["is_connection"].as_bool().unwrap_or(false);
            let timestamp = normalize_ts(p["timestamp"].as_u64().unwrap_or(0));
            let media_urls: Vec<String> = p["media"]
                .as_array()
                .map(|a| a.iter().filter_map(|m| m.as_str().map(String::from)).collect())
                .unwrap_or_default();

            let poster = media_urls.first().cloned();
            posts.push(Post {
                id,
                platform: Platform::LinkedIn,
                author_id,
                author_username,
                content,
                media_urls,
                poster_url: poster,
                liker_ids: Vec::new(),
                commenter_ids: Vec::new(),
                timestamp,
                is_video,
                author_is_mutual: Some(is_connection),
                author_is_close_friend: None,
                engagement_score: None,
                is_synthetic: None,
                vector_embedding: None,
            });
        }
        posts
    }

    fn voyager_headers(&self, csrf: &str) -> Vec<(&'static str, String)> {
        vec![
            ("Accept", "application/vnd.linkedin.normalized+json+2.1".to_string()),
            ("Csrf-Token", csrf.to_string()),
            ("X-RestLi-Protocol-Version", "2.0.0".to_string()),
            ("X-Requested-With", "XMLHttpRequest".to_string()),
        ]
    }

    async fn get_voyager(&self, client: &HttpClient, url: &str, csrf: &str, referer: &str) -> Result<serde_json::Value, String> {
        let extra = self.voyager_headers(csrf);
        client.get_json_headers(url, Some(referer), &extra).await
    }
}

#[cfg(test)]
fn cred(session: &str) -> Credential {
    Credential {
        platform: Platform::LinkedIn,
        session_token: session.to_string(),
        user_id: "123".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ing() -> LinkedInIngester {
        LinkedInIngester
    }

    #[test]
    fn test_csrf_token_from_jsessionid() {
        let c = cred("li_at=abc123; JSESSIONID=\"ajax:987654\"; lang=en");
        assert_eq!(ing().csrf_token(&c), "ajax:987654");
    }

    #[test]
    fn test_csrf_token_missing() {
        let c = cred("li_at=abc123; some=x");
        assert_eq!(ing().csrf_token(&c), "");
    }

    #[test]
    fn test_auth_header_bearer() {
        let c = cred("li_at=abc123; JSESSIONID=x; lang=en");
        assert_eq!(ing().auth_header(&c), "Bearer abc123");
    }

    #[test]
    fn test_auth_header_empty() {
        let c = cred("JSESSIONID=x; lang=en");
        assert_eq!(ing().auth_header(&c), "Bearer ");
    }

    #[test]
    fn test_normalize_ts_seconds_passthrough() {
        assert_eq!(normalize_ts(1_700_000_000u64), 1_700_000_000u64);
    }

    #[test]
    fn test_normalize_ts_millis_to_seconds() {
        assert_eq!(normalize_ts(1_700_000_000_000u64), 1_700_000_000u64);
    }

    #[test]
    fn test_parse_text_activity() {
        let activity = serde_json::json!({
            "$urn": "urn:li:activity:7123456789012345678",
            "summary": { "text": "Just shipped the new dashboard board." },
            "actor": {
                "$urn": "urn:li:person:901",
                "name": "Jane Doe",
                "miniProfile": { "publicIdentifier": "janedoe" }
            },
            "temporal": { "time": 1_700_000_000_000i64 }
        });

        let post = ing().parse_activity(&activity).expect("should parse");
        assert_eq!(post.platform, Platform::LinkedIn);
        assert_eq!(post.id, "7123456789012345678");
        assert_eq!(post.author_id, "901");
        assert_eq!(post.author_username, "Jane Doe");
        assert_eq!(post.content, "Just shipped the new dashboard board.");
        assert_eq!(post.timestamp, 1_700_000_000u64);
        assert!(!post.is_video);
        assert!(post.media_urls.is_empty());
    }

    #[test]
    fn test_parse_activity_commentary_fallback() {
        let activity = serde_json::json!({
            "urn": "urn:li:activity:123",
            "commentary": { "text": "Morning thoughts" },
            "actor": { "urn": "urn:li:person:4", "name": "Bob" },
            "temporal": { "time": 1_700_000_000_000i64 }
        });
        let post = ing().parse_activity(&activity).expect("should parse");
        assert_eq!(post.content, "Morning thoughts");
        assert_eq!(post.timestamp, 1_700_000_000u64);
    }

    #[test]
    fn test_parse_image_activity_media_and_likers() {
        let activity = serde_json::json!({
            "$urn": "urn:li:activity:456",
            "headline": { "text": "Some image" },
            "actor": {
                "$urn": "urn:li:person:100",
                "miniProfile": { "publicIdentifier": "alice" }
            },
            "temporal": { "time": 1_700_000_000_000i64 },
            "images": [
                { "url": "https://cdn.example.com/a.jpg" },
                { "attributes": [ { "detailData": { "url": "https://cdn.example.com/b.jpg" } } ] }
            ],
            "likes": {
                "elements": [
                    { "actor": { "$urn": "urn:li:person:201" } },
                    { "actor": { "$urn": "urn:li:person:202" } }
                ]
            }
        });

        let post = ing().parse_activity(&activity).unwrap();
        assert_eq!(post.media_urls, vec![
            "https://cdn.example.com/a.jpg",
            "https://cdn.example.com/b.jpg"
        ]);
        assert_eq!(post.liker_ids, vec!["201", "202"]);
        assert_eq!(post.author_username, "alice");
        assert!(!post.is_video);
    }

    #[test]
    fn test_parse_video_activity() {
        let activity = serde_json::json!({
            "$urn": "urn:li:activity:789",
            "commentary": { "text": "reel time" },
            "actor": { "$urn": "urn:li:person:300", "name": "Carol" },
            "temporal": { "time": 1_700_000_000_000i64 },
            "content": {
                "type": "video",
                "playlists": [ { "url": "https://cdn.example.com/video.mp4" } ]
            }
        });

        let post = ing().parse_activity(&activity).expect("should parse");
        assert!(post.is_video);
        assert_eq!(post.media_urls, vec!["https://cdn.example.com/video.mp4"]);
    }

    #[test]
    fn test_parse_activity_missing_urn() {
        assert!(ing().parse_activity(&serde_json::json!({ "summary": { "text": "x" } })).is_none());
    }

    #[test]
    fn test_parse_feed_items_dedupes_across_sections() {
        let body = serde_json::json!({
            "data": {
                "feedDashUrs": {
                    "elements": [
                        { "activity": { "$urn": "urn:li:activity:1", "summary": { "text": "one" }, "actor": { "$urn": "urn:li:person:1", "name": "a" }, "temporal": { "time": 1 } } },
                        { "activity": { "$urn": "urn:li:activity:2", "summary": { "text": "two" }, "actor": { "$urn": "urn:li:person:1", "name": "a" }, "temporal": { "time": 1 } } }
                    ]
                }
            },
            "included": [
                { "activity": { "$urn": "urn:li:activity:1", "summary": { "text": "dup" }, "actor": { "$urn": "urn:li:person:1", "name": "a" }, "temporal": { "time": 1 } } },
                { "activity": { "$urn": "urn:li:activity:3", "summary": { "text": "three" }, "actor": { "$urn": "urn:li:person:1", "name": "a" }, "temporal": { "time": 1 } } }
            ]
        });

        let posts = ing().parse_feed_items(&body);
        let ids: Vec<String> = posts.iter().map(|p| p.id.clone()).collect();
        assert_eq!(ids, vec!["1", "2", "3"]);
    }

    #[test]
    fn test_parse_feed_items_empty() {
        assert!(ing().parse_feed_items(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn test_extract_profile() {
        let body = serde_json::json!({
            "data": {
                "user": {
                    "urn": "urn:li:person:77",
                    "publicIdentifier": "janedoe",
                    "followers": { "elements": [ { "urn": "urn:li:person:100" } ] },
                    "following": { "elements": [ { "urn": "urn:li:person:200" }, { "urn": "urn:li:person:201" } ] }
                }
            }
        });
        let user = ing().extract_profile(&body);
        assert_eq!(user.id, "77");
        assert_eq!(user.platform, Platform::LinkedIn);
        assert_eq!(user.username, "janedoe");
        assert_eq!(user.followers, vec!["100"]);
        assert_eq!(user.follows, vec!["200", "201"]);
    }

    #[test]
    fn test_extract_messages_parses_events() {
        let body = serde_json::json!({
            "data": {
                "conversations": {
                    "elements": [
                        {
                            "$urn": "urn:li:messageThread:abc",
                            "events": [
                                { "$id": "ev1", "content": { "text": "hey" }, "from": { "urn": "urn:li:person:500" }, "createdAt": 1_700_000_000_000i64 },
                                { "$id": "ev2", "body": "whats up", "from": { "urn": "urn:li:person:77" }, "createdAt": 1_700_000_100_000i64 }
                            ]
                        }
                    ]
                }
            }
        });

        let msgs = ing().extract_messages(&body);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].platform, Platform::LinkedIn);
        assert_eq!(msgs[0].conversation_id, "urn:li:messageThread:abc");
        assert_eq!(msgs[0].sender_id, "500");
        assert_eq!(msgs[0].content, "hey");
        assert_eq!(msgs[0].timestamp, 1_700_000_000u64);
        assert_eq!(msgs[1].content, "whats up");
        assert_eq!(msgs[1].sender_id, "77");
    }

    #[test]
    fn test_extract_messages_body_fallback() {
        let body = serde_json::json!({
            "data": { "conversations": { "elements": [
                { "entityUrn": "urn:li:messageThread:x", "events": [
                    { "$id": "e1", "body": "msg body only", "from": { "urn": "urn:li:person:1" }, "createdAt": 1000 }
                ] }
            ] } }
        });
        let msgs = ing().extract_messages(&body);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "msg body only");
        assert_eq!(msgs[0].conversation_id, "urn:li:messageThread:x");
    }

    #[test]
    fn test_extract_messages_empty() {
        assert!(ing().extract_messages(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn test_feed_query_id_from_text() {
        assert_eq!(
            feed_query_id_from_text(
                "window.__voyagerFeedDashMainFeedQueryId__='voyagerFeedDashMainFeed.7a50ef8ba5a7865c23ad5df46f735709';"
            ),
            Some("7a50ef8ba5a7865c23ad5df46f735709".to_string())
        );
        assert_eq!(feed_query_id_from_text("no query id here"), None);
    }

    #[test]
    fn test_parse_graphql_feed_resolves_refs() {
        let body = serde_json::json!({
            "data": { "data": { "feedDashMainFeedByMainFeed": {
                "*elements": [
                    "urn:li:activity:111",
                    "urn:li:activity:222",
                    { "entityUrn": "urn:li:activity:333", "activity": {
                        "$urn": "urn:li:activity:333",
                        "summary": { "text": "inline element" },
                        "temporal": { "time": 1_700_000_000_000i64 }
                    } }
                ]
            } } },
            "included": [
                { "$urn": "urn:li:activity:111", "entityUrn": "urn:li:activity:111",
                  "updateV2": {
                      "commentary": { "commentary": { "text": "graphql post one" } },
                      "actor": { "$urn": "urn:li:person:500", "miniProfile": { "publicIdentifier": "alice" } },
                      "content": { "images": [ { "url": "https://cdn.example/i1.jpg" } ] },
                      "*metadata": { "time": 1_700_000_000_000i64 }
                  } },
                { "$urn": "urn:li:activity:222",
                  "updateV2": {
                      "commentary": { "text": "graphql post two" },
                      "actor": "urn:li:person:77",
                      "content": { "type": "video", "playlists": [ { "url": "https://cdn.example/v.mp4" } ] },
                      "*metadata": { "time": 1_700_000_100_000i64 }
                  } },
                { "$urn": "urn:li:person:500", "miniProfile": { "publicIdentifier": "alice" } }
            ]
        });

        let posts = ing().parse_graphql_feed(&body);
        assert_eq!(posts.len(), 3);

        let one = posts.iter().find(|p| p.id == "111").unwrap();
        assert_eq!(one.content, "graphql post one");
        assert_eq!(one.author_id, "500");
        assert_eq!(one.author_username, "alice");
        assert_eq!(one.timestamp, 1_700_000_000u64);
        assert_eq!(one.media_urls, vec!["https://cdn.example/i1.jpg"]);
        assert_eq!(one.is_video, false);

        let two = posts.iter().find(|p| p.id == "222").unwrap();
        assert_eq!(two.content, "graphql post two");
        assert_eq!(two.is_video, true);
        assert_eq!(two.media_urls, vec!["https://cdn.example/v.mp4"]);

        let three = posts.iter().find(|p| p.id == "333").unwrap();
        assert_eq!(three.content, "inline element");
    }

    #[test]
    fn test_parse_graphql_feed_empty() {
        assert!(ing().parse_graphql_feed(&serde_json::json!({})).is_empty());
    }
}
