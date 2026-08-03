use async_trait::async_trait;

use crate::http::HttpClient;
use crate::types::{Credential, Message, Platform, Post, SocialUser};
use super::PlatformIngester;

const API_BASE: &str = "https://www.linkedin.com/voyager/api";

fn normalize_ts(raw: u64) -> u64 {
    if raw > 1_000_000_000_000 {
        raw / 1_000
    } else {
        raw
    }
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
        let client = self.build_client(credential);
        let csrf = self.csrf_token(credential);

        let url = format!(
            "{}/feed/dashUpdates?start=0&count=10&feedType=ALL&feedModuleType=HYPE_FEED&csrfToken={}",
            API_BASE,
            urlencoding::encode(&csrf)
        );

        let body = self.get_voyager(&client, &url, &csrf, "https://www.linkedin.com/feed/").await?;
        let posts = self.parse_feed_items(&body);

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

        let body = self.get_voyager(&client, &url, &csrf, "https://www.linkedin.com/messaging/").await?;
        let msgs = self.extract_messages(&body);

        Ok(msgs)
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
}
