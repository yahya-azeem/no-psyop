use async_trait::async_trait;
use std::collections::HashMap;
use crate::http::xproxy::XProxy;
use crate::types::{Credential, Message, Platform, Post, SocialUser};
use super::PlatformIngester;

pub struct TwitterIngester;

impl TwitterIngester {
    fn extract_tweets(&self, body: &serde_json::Value) -> Vec<Post> {
        let mut posts = Vec::new();

        // Real HomeTimeline shape (captured via Playwright): snake_case with trailing `t`.
        if let Some(instructions) = body["data"]["home"]["home_timeline_urt"]["instructions"].as_array() {
            for instruction in instructions {
                if let Some(entries) = instruction["entries"].as_array() {
                    for entry in entries {
                        let result = &entry["content"]["itemContent"]["tweet_results"]["result"];
                        if !result.is_null() {
                            if let Some(post) = self.parse_tweet(result) {
                                posts.push(post);
                            }
                        }
                    }
                }
            }
        }

        if let Some(entries) = body["data"]["home"]["home_timeline_ur"]["instructions"].as_array() {
            for instruction in entries {
                if let Some(entries) = instruction["entries"].as_array() {
                    for entry in entries {
                        let result = &entry["content"]["itemContent"]["tweet_results"]["result"];
                        if !result.is_null() {
                            if let Some(post) = self.parse_tweet(result) {
                                posts.push(post);
                            }
                        }
                    }
                }
            }
        }

        if let Some(instructions) = body["data"]["home"]["homeTimelineUrt"]["instructions"].as_array() {
            for instr in instructions {
                if let Some(add_entries) = instr["addEntries"]["entries"].as_array() {
                    for entry in add_entries {
                        let result = &entry["content"]["itemContent"]["tweet_results"]["result"];
                        if !result.is_null() {
                            if let Some(post) = self.parse_tweet(result) {
                                posts.push(post);
                            }
                        }
                    }
                }
            }
        }

        if let Some(global_objects) = body["globalObjects"]["tweets"].as_object() {
            for (_id, tweet) in global_objects {
                if let Some(post) = self.parse_legacy_tweet(tweet) {
                    posts.push(post);
                }
            }
        }

        posts
    }

    fn parse_tweet(&self, result: &serde_json::Value) -> Option<Post> {
        let legacy = result.get("legacy")?;
        let id = result["rest_id"]
            .as_str()
            .or_else(|| legacy["id_str"].as_str())?
            .to_string();
        let text = legacy["full_text"]
            .as_str()
            .or_else(|| legacy["text"].as_str())
            .unwrap_or("")
            .to_string();

        let user = &result["core"]["user_results"]["result"];
        let user_id = user["rest_id"]
            .as_str()
            .or_else(|| legacy["user_id_str"].as_str())
            .unwrap_or("")
            .to_string();
        let username = user["core"]["screen_name"]
            .as_str()
            .or_else(|| legacy["screen_name"].as_str())
            .unwrap_or("")
            .to_string();
        let timestamp = created_at_ts(&result["legacy"]);

        let (media_urls, is_video) = extract_media(legacy);

        let liker_ids: Vec<String> = legacy["favorited_by"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let commenter_ids: Vec<String> = legacy["reply_count"]
            .as_i64()
            .map(|_| Vec::new())
            .unwrap_or_default();

        Some(Post {
            id,
            platform: Platform::Twitter,
            author_id: user_id,
            author_username: username,
            content: text,
            media_urls,
            poster_url: None,
            liker_ids,
            commenter_ids,
            timestamp,
            is_video,
            author_is_mutual: None,
            author_is_close_friend: None,
            engagement_score: None,
            is_synthetic: None,
            vector_embedding: None,
        })
    }

    fn parse_legacy_tweet(&self, tweet: &serde_json::Value) -> Option<Post> {
        let id_str = tweet["id_str"].as_str()?;
        let text = tweet["full_text"].as_str()
            .or_else(|| tweet["text"].as_str())
            .unwrap_or("")
            .to_string();

        let user_id = tweet["user_id_str"].as_str().unwrap_or("").to_string();
        let username = tweet["screen_name"].as_str().unwrap_or("").to_string();
        let timestamp = created_at_ts(tweet);
        let (media_urls, is_video) = extract_media(tweet);

        let liker_ids: Vec<String> = tweet["favorited_by"].as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let commenter_ids: Vec<String> = tweet["reply_count"].as_i64()
            .map(|_| Vec::new())
            .unwrap_or_default();

        Some(Post {
            id: id_str.to_string(),
            platform: Platform::Twitter,
            author_id: user_id,
            author_username: username,
            content: text,
            media_urls,
            poster_url: None,
            liker_ids,
            commenter_ids,
            timestamp,
            is_video,
            author_is_mutual: None,
            author_is_close_friend: None,
            engagement_score: None,
            is_synthetic: None,
            vector_embedding: None,
        })
    }

    fn extract_user_id(&self, body: &serde_json::Value) -> String {
        body["data"]["user"]["result"]["rest_id"]
            .as_str()
            .or_else(|| body["data"]["user"]["rest_id"].as_str())
            .or_else(|| body["data"]["user"]["id_str"].as_str())
            .unwrap_or("")
            .to_string()
    }

    fn extract_username(&self, body: &serde_json::Value) -> String {
        body["data"]["user"]["result"]["core"]["screen_name"]
            .as_str()
            .or_else(|| body["data"]["user"]["result"]["legacy"]["screen_name"].as_str())
            .or_else(|| body["data"]["user"]["legacy"]["screen_name"].as_str())
            .or_else(|| body["data"]["user"]["screen_name"].as_str())
            .unwrap_or("")
            .to_string()
    }

    fn extract_followers(&self, body: &serde_json::Value) -> Vec<crate::types::UserId> {
        let mut ids = Vec::new();
        if let Some(entries) = body["data"]["user"]["followers"]["timeline"]["instructions"]
            .as_array().and_then(|i| i.first())
            .and_then(|i| i["addEntries"]["entries"].as_array())
        {
            for entry in entries {
                if let Some(id) = entry["content"]["itemContent"]["user"]["rest_id"].as_str() {
                    ids.push(id.to_string());
                }
            }
        }
        ids
    }

    fn extract_following(&self, body: &serde_json::Value) -> Vec<crate::types::UserId> {
        let mut ids = Vec::new();
        if let Some(entries) = body["data"]["user"]["following"]["timeline"]["instructions"]
            .as_array().and_then(|i| i.first())
            .and_then(|i| i["addEntries"]["entries"].as_array())
        {
            for entry in entries {
                if let Some(id) = entry["content"]["itemContent"]["user"]["rest_id"].as_str() {
                    ids.push(id.to_string());
                }
            }
        }
        ids
    }

    fn extract_messages(&self, body: &serde_json::Value) -> Vec<Message> {
        let mut users: HashMap<String, String> = HashMap::new();
        let mut msgs: Vec<Message> = Vec::new();

        if let Some(entries) = body["data"]["dm_inbox_timeline"]["timeline"]["instructions"]
            .as_array().and_then(|i| i.first())
            .and_then(|i| i["addEntries"]["entries"].as_array())
        {
            for entry in entries {
                if let Some(participants) = entry["content"]["itemContent"]["participants"].as_array() {
                    for p in participants {
                        let res = &p["user_results"]["result"];
                        if let (Some(id), Some(name)) = (
                            res["rest_id"].as_str(),
                            res["legacy"]["screen_name"].as_str(),
                        ) {
                            users.insert(id.to_string(), name.to_string());
                        }
                    }
                }
                if let Some(msg) = entry["content"]["itemContent"]["message"].as_object() {
                    let id = msg["id"].as_str().unwrap_or("").to_string();
                    let text = msg["text"].as_str().unwrap_or("").to_string();
                    let sender_raw = msg["sender_id"].as_str().unwrap_or("").to_string();
                    let sender = users
                        .get(&sender_raw)
                        .cloned()
                        .unwrap_or_else(|| sender_raw.clone());
                    let conv = msg["conversation_id"].as_str().unwrap_or("").to_string();
                    let ts = msg["time"].as_i64().unwrap_or(0) as u64;

                    msgs.push(Message {
                        id,
                        platform: Platform::Twitter,
                        conversation_id: conv,
                        sender_id: sender,
                        content: text,
                        timestamp: ts,
                    });
                }
            }
        }

        msgs
    }
}

/// Seconds since epoch. Real GraphQL tweets carry a `created_at` string like
/// "Sun Aug 02 21:29:26 +0000 2026"; legacy shapes carry `timestamp_ms`.
fn created_at_ts(tweet: &serde_json::Value) -> u64 {
    if let Some(s) = tweet["created_at"].as_str() {
        if let Ok(dt) = chrono::DateTime::parse_from_str(s, "%a %b %d %H:%M:%S %z %Y") {
            return dt.timestamp() as u64;
        }
    }
    (tweet["timestamp_ms"]
        .as_i64()
        .or_else(|| tweet["timestamp_ms"].as_str().and_then(|s| s.parse::<i64>().ok()))
        .unwrap_or(0)
        / 1000) as u64
}

fn extract_media(tweet: &serde_json::Value) -> (Vec<String>, bool) {
    let is_video = tweet["extended_entities"]["media"]
        .as_array()
        .map(|a| a.iter().any(|m| m["type"].as_str() == Some("video")))
        .unwrap_or(false);

    let mut media_urls = Vec::new();
    if let Some(media) = tweet["extended_entities"]["media"].as_array() {
        for m in media {
            if let Some(url) = m["media_url_https"].as_str().or(m["media_url"].as_str()) {
                media_urls.push(url.to_string());
            }
        }
    }
    if media_urls.is_empty() {
        if let Some(media) = tweet["entities"]["media"].as_array() {
            for m in media {
                if let Some(url) = m["media_url_https"].as_str().or(m["media_url"].as_str()) {
                    media_urls.push(url.to_string());
                }
            }
        }
    }
    (media_urls, is_video)
}

#[async_trait]
impl PlatformIngester for TwitterIngester {
    fn platform(&self) -> Platform {
        Platform::Twitter
    }

    async fn fetch_feed(&mut self, credential: &Credential) -> Result<Vec<Post>, String> {
        let body = XProxy::feed(&credential.session_token).await?;
        Ok(self.extract_tweets(&body))
    }

    async fn fetch_profile(&mut self, credential: &Credential, username: &str) -> Result<SocialUser, String> {
        let body = XProxy::profile(&credential.session_token, username).await?;

        let id = self.extract_user_id(&body);
        let name = self.extract_username(&body);
        let followers = self.extract_followers(&body);
        let following = self.extract_following(&body);

        Ok(SocialUser {
            id,
            platform: Platform::Twitter,
            username: name,
            follows: following,
            followers,
            last_sync: chrono::Utc::now().timestamp() as u64,
        })
    }

    async fn fetch_messages(&mut self, credential: &Credential) -> Result<Vec<Message>, String> {
        let body = XProxy::inbox(&credential.session_token).await?;
        if body["empty_inbox"].as_bool().unwrap_or(false) {
            return Ok(Vec::new());
        }
        Ok(self.extract_messages(&body))
    }

    async fn refresh_session(&mut self, credential: &Credential) -> Result<Credential, String> {
        Ok(credential.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ing() -> TwitterIngester {
        TwitterIngester
    }

    fn legacy_tweet(id: &str, text: &str, user_id: &str, screen_name: &str, ts_ms: u64) -> serde_json::Value {
        serde_json::json!({
            "id_str": id,
            "full_text": text,
            "user_id_str": user_id,
            "screen_name": screen_name,
            "timestamp_ms": ts_ms.to_string(),
            "entities": { "media": [] },
            "extended_entities": { "media": [] },
        })
    }

    #[test]
    fn test_parse_legacy_tweet_text() {
        let tweet = legacy_tweet("1", "hello world", "500", "alice", 1_700_000_000_000);
        let post = ing().parse_legacy_tweet(&tweet).expect("parsed");
        assert_eq!(post.id, "1");
        assert_eq!(post.platform, Platform::Twitter);
        assert_eq!(post.author_id, "500");
        assert_eq!(post.author_username, "alice");
        assert_eq!(post.timestamp, 1_700_000_000u64);
        assert!(!post.is_video);
        assert!(post.media_urls.is_empty());
    }

    #[test]
    fn test_parse_legacy_tweet_media_video() {
        let tweet = serde_json::json!({
            "id_str": "2",
            "full_text": "clip",
            "user_id_str": "77",
            "screen_name": "bob",
            "timestamp_ms": 1000,
            "extended_entities": {
                "media": [
                    { "type": "video", "media_url_https": "https://pbs.twimg.com/v.mp4" },
                    { "type": "photo", "media_url_https": "https://pbs.twimg.com/p.jpg" }
                ]
            },
            "favorited_by": ["100", "101"]
        });
        let post = ing().parse_legacy_tweet(&tweet).expect("parsed");
        assert!(post.is_video);
        assert_eq!(post.media_urls, vec![
            "https://pbs.twimg.com/v.mp4",
            "https://pbs.twimg.com/p.jpg"
        ]);
        assert_eq!(post.liker_ids, vec!["100", "101"]);
    }

    #[test]
    fn test_parse_legacy_tweet_missing_id() {
        assert!(ing().parse_legacy_tweet(&serde_json::json!({})).is_none());
    }

    #[test]
    fn test_extract_tweets_home_timeline_urt_snake() {
        let body = serde_json::json!({
            "data": { "home": { "home_timeline_urt": { "instructions": [
                { "type": "TimelineAddEntries", "entries": [
                    { "content": { "itemContent": { "tweet_results": { "result": {
                        "legacy": legacy_tweet("10", "first", "1", "u1", 1_000_000)
                    } } } } },
                    { "content": { "itemContent": { "tweet_results": { "result": {
                        "legacy": legacy_tweet("11", "second", "2", "u2", 2_000_000)
                    } } } } }
                ] }
            ] } } }
        });
        let posts = ing().extract_tweets(&body);
        assert_eq!(posts.len(), 2);
        assert_eq!(posts[0].id, "10");
        assert_eq!(posts[1].content, "second");
    }

    #[test]
    fn test_extract_tweets_home_timeline_ur() {
        let body = serde_json::json!({
            "data": { "home": { "home_timeline_ur": { "instructions": [
                { "type": "TimelineAddEntries", "entries": [
                    { "content": { "itemContent": { "tweet_results": { "result": {
                        "legacy": legacy_tweet("10", "first", "1", "u1", 1_000_000)
                    } } } } }
                ] }
            ] } } }
        });
        let posts = ing().extract_tweets(&body);
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].id, "10");
    }

    #[test]
    fn test_extract_tweets_home_timeline_urt_camel() {
        let body = serde_json::json!({
            "data": { "home": { "homeTimelineUrt": { "instructions": [
                { "addEntries": { "entries": [
                    { "content": { "itemContent": { "tweet_results": { "result": {
                        "legacy": legacy_tweet("20", "urt post", "3", "u3", 3_000_000)
                    } } } } }
                ] } }
            ] } } }
        });
        let posts = ing().extract_tweets(&body);
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].id, "20");
        assert_eq!(posts[0].author_username, "u3");
    }

    #[test]
    fn test_extract_tweets_global_objects() {
        let body = serde_json::json!({
            "globalObjects": { "tweets": {
                "30": legacy_tweet("30", "legacy objects", "4", "u4", 4_000_000)
            } }
        });
        let posts = ing().extract_tweets(&body);
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].content, "legacy objects");
    }

    #[test]
    fn test_extract_tweets_empty() {
        assert!(ing().extract_tweets(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn test_extract_user_from_result_shape() {
        let body = serde_json::json!({
            "data": { "user": { "result": {
                "rest_id": "1678652827302862848",
                "core": { "screen_name": "realname" },
                "legacy": { "screen_name": "legacyname" }
            } } }
        });
        assert_eq!(ing().extract_user_id(&body), "1678652827302862848");
        assert_eq!(ing().extract_username(&body), "realname");
    }

    #[test]
    fn test_extract_user_result_legacy_fallback() {
        let body = serde_json::json!({
            "data": { "user": { "result": {
                "rest_id": "42",
                "legacy": { "screen_name": "oldname" }
            } } }
        });
        assert_eq!(ing().extract_username(&body), "oldname");
    }

    #[test]
    fn test_extract_user_empty() {
        assert!(ing().extract_user_id(&serde_json::json!({})).is_empty());
        assert!(ing().extract_username(&serde_json::json!({})).is_empty());
    }

    fn dm_entry(conv: &str, msg_id: &str, text: &str, sender: &str, participants: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "entryId": format!("dm-conversation-{}", conv),
            "content": { "itemContent": {
                "conversationId": conv,
                "message": {
                    "id": msg_id,
                    "text": text,
                    "sender_id": sender,
                    "conversation_id": conv,
                    "time": 5_000_000i64
                },
                "participants": participants
            } }
        })
    }

    #[test]
    fn test_extract_messages_resolves_usernames() {
        let participants = serde_json::json!([
            { "user_results": { "result": { "rest_id": "500", "legacy": { "screen_name": "alice" } } } },
            { "user_results": { "result": { "rest_id": "77", "legacy": { "screen_name": "bob" } } } }
        ]);
        let body = serde_json::json!({
            "data": { "dm_inbox_timeline": { "timeline": { "instructions": [
                { "addEntries": { "entries": [
                    dm_entry("c1", "m1", "hey", "500", participants)
                ] } }
            ] } } }
        });

        let msgs = ing().extract_messages(&body);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].platform, Platform::Twitter);
        assert_eq!(msgs[0].conversation_id, "c1");
        assert_eq!(msgs[0].sender_id, "alice");
        assert_eq!(msgs[0].content, "hey");
        assert_eq!(msgs[0].timestamp, 5_000_000u64);
    }

    #[test]
    fn test_extract_messages_falls_back_to_raw_id() {
        let body = serde_json::json!({
            "data": { "dm_inbox_timeline": { "timeline": { "instructions": [
                { "addEntries": { "entries": [
                    dm_entry("c2", "m2", "hi", "999", serde_json::json!([]))
                ] } }
            ] } } }
        });
        let msgs = ing().extract_messages(&body);
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].sender_id, "999");
    }

    #[test]
    fn test_extract_messages_empty() {
        assert!(ing().extract_messages(&serde_json::json!({})).is_empty());
    }
}
