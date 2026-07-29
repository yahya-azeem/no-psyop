use async_trait::async_trait;
use crate::http::HttpClient;
use crate::types::{Credential, Message, Platform, Post, SocialUser};
use super::PlatformIngester;

pub struct InstagramIngester;

impl InstagramIngester {
    fn user_agent(&self) -> String {
        "Mozilla/5.0 (Linux; Android 14; Pixel 8 Pro) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.6613.146 Mobile Safari/537.36".into()
    }

    async fn extract_csrf(&self, client: &HttpClient) -> Result<String, String> {
        let resp = client.client()
            .get("https://www.instagram.com/api/v1/web/accounts/login/")
            .header("User-Agent", self.user_agent())
            .send()
            .await
            .map_err(|e| format!("csrf fetch failed: {}", e))?;

        let csrf = resp
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| {
                s.split(';').find(|p| p.trim().starts_with("csrftoken="))
                    .and_then(|p| p.split('=').nth(1))
            })
            .map(|s| s.to_string())
            .unwrap_or_default();

        Ok(csrf)
    }

    fn parse_feed_items(&self, body: &serde_json::Value) -> Vec<Post> {
        let mut posts = Vec::new();
        let reels = body["data"]["reels_media"].as_array();

        if let Some(reels) = reels {
            for reel in reels {
                if let Some(items) = reel["items"].as_array() {
                    for item in items {
                        if let Some(post) = self.parse_item(item) {
                            posts.push(post);
                        }
                    }
                }
            }
            return posts;
        }

        if let Some(edges) = body["data"]["user"]["edge_web_feed_timeline"]["edges"].as_array() {
            for edge in edges {
                if let Some(node) = edge["node"].as_object() {
                    if let Some(shortcode) = node.get("shortcode").and_then(|v| v.as_str()) {
                        let text = node
                            .get("edge_media_to_caption")
                            .and_then(|c| c["edges"].as_array())
                            .and_then(|e| e.first())
                            .and_then(|e| e["node"]["text"].as_str())
                            .unwrap_or("")
                            .to_string();

                        let likers: Vec<String> = node
                            .get("edge_media_preview_like")
                            .and_then(|l| l["edges"].as_array())
                            .map(|edges| {
                                edges.iter()
                                    .filter_map(|e| e["node"]["id"].as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();

                        let commenters: Vec<String> = node
                            .get("edge_media_to_comment")
                            .and_then(|c| c["edges"].as_array())
                            .map(|edges| {
                                edges.iter()
                                    .filter_map(|e| e["node"]["owner"]["id"].as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default();

                        let is_video = node.get("is_video").and_then(|v| v.as_bool()).unwrap_or(false);

                        let mut media_urls = Vec::new();
                        if is_video {
                            if let Some(src) = node.get("video_url").and_then(|v| v.as_str()) {
                                media_urls.push(src.to_string());
                            }
                        } else {
                            if let Some(src) = node.get("display_url").and_then(|v| v.as_str()) {
                                media_urls.push(src.to_string());
                            }
                        }

                        let author_id = node
                            .get("owner")
                            .and_then(|o| o["id"].as_str())
                            .unwrap_or("")
                            .to_string();

                        let author_username = node
                            .get("owner")
                            .and_then(|o| o["username"].as_str())
                            .unwrap_or("")
                            .to_string();

                        let timestamp = node
                            .get("taken_at_timestamp")
                            .and_then(|t| t.as_i64())
                            .unwrap_or(0) as u64;

                        posts.push(Post {
                            id: shortcode.to_string(),
                            platform: Platform::Instagram,
                            author_id,
                            author_username,
                            content: text,
                            media_urls,
                            liker_ids: likers,
                            commenter_ids: commenters,
                            timestamp,
                            is_video,
                            engagement_score: None,
                            is_synthetic: None,
                            vector_embedding: None,
                        });
                    }
                }
            }
        }

        posts
    }

    fn parse_item(&self, item: &serde_json::Value) -> Option<Post> {
        let id = item["id"].as_str()?.to_string();
        let text = item["caption"]["text"].as_str().unwrap_or("").to_string();
        let code = item["code"].as_str().unwrap_or(&id);

        let author_id = item["user"]["pk"].as_i64().map(|i| i.to_string()).unwrap_or_default();
        let author_username = item["user"]["username"].as_str().unwrap_or("").to_string();

        let is_video = item["media_type"].as_i64().unwrap_or(0) == 2;
        let mut media_urls = Vec::new();
        if is_video {
            if let Some(src) = item["video_versions"].as_array().and_then(|a| a.first()).and_then(|v| v["url"].as_str()) {
                media_urls.push(src.to_string());
            }
        } else {
            if let Some(src) = item["image_versions2"]["candidates"].as_array().and_then(|a| a.first()).and_then(|c| c["url"].as_str()) {
                media_urls.push(src.to_string());
            }
        }

        let timestamp = item["taken_at"].as_i64().unwrap_or(0) as u64;

        Some(Post {
            id: code.to_string(),
            platform: Platform::Instagram,
            author_id,
            author_username,
            content: text,
            media_urls,
            liker_ids: Vec::new(),
            commenter_ids: Vec::new(),
            timestamp,
            is_video,
            engagement_score: None,
            is_synthetic: None,
            vector_embedding: None,
        })
    }

    fn extract_username(&self, body: &serde_json::Value) -> String {
        body["data"]["user"]["username"]
            .as_str()
            .or_else(|| body["user"]["username"].as_str())
            .unwrap_or("")
            .to_string()
    }

    fn extract_user_id(&self, body: &serde_json::Value) -> String {
        body["data"]["user"]["id"]
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| body["user"]["pk"].as_i64().map(|i| i.to_string()))
            .unwrap_or_default()
    }

    fn extract_followers(&self, body: &serde_json::Value) -> Vec<crate::types::UserId> {
        let mut ids = Vec::new();
        if let Some(edges) = body["data"]["user"]["edge_followed_by"]["edges"].as_array() {
            for edge in edges {
                if let Some(id) = edge["node"]["id"].as_str() {
                    ids.push(id.to_string());
                }
            }
        }
        ids
    }

    fn extract_following(&self, body: &serde_json::Value) -> Vec<crate::types::UserId> {
        let mut ids = Vec::new();
        if let Some(edges) = body["data"]["user"]["edge_follow"]["edges"].as_array() {
            for edge in edges {
                if let Some(id) = edge["node"]["id"].as_str() {
                    ids.push(id.to_string());
                }
            }
        }
        ids
    }

    async fn fetch_graphql(&self, client: &HttpClient, query_hash: &str, variables: &serde_json::Value) -> Result<serde_json::Value, String> {
        let vars = serde_json::to_string(variables).map_err(|e| e.to_string())?;
        let url = format!(
            "https://www.instagram.com/graphql/query/?query_hash={}&variables={}",
            query_hash, urlencoding(&vars)
        );
        client.get_json(&url, Some("https://www.instagram.com/")).await
    }
}

#[async_trait]
impl PlatformIngester for InstagramIngester {
    fn platform(&self) -> Platform {
        Platform::Instagram
    }

    async fn fetch_feed(&mut self, credential: &Credential) -> Result<Vec<Post>, String> {
        let mut client = HttpClient::with_session(&credential.session_token);

        let feed_query_hash = "485b270a140e96e6baf6e36cdf20aa3c";
        let variables = serde_json::json!({
            "fetch_media_item_count": 12,
            "fetch_media_item_cursor": "",
            "fetch_comment_count": 4,
            "fetch_like_count": 4,
            "has_stories": false,
        });

        let body = self.fetch_graphql(&mut client, feed_query_hash, &variables).await?;

        let mut posts = self.parse_feed_items(&body);

        let reels_query_hash = "b3055c01b4b222b8a508dc29812deb77";
        let reels_vars = serde_json::json!({
            "reel_ids": [credential.user_id],
            "tag_names": [],
            "location_ids": [],
            "highlight_reel_ids": [],
            "precomposed_overlay": false,
            "show_story_viewer_list": false,
        });

        if let Ok(stories_body) = self.fetch_graphql(&mut client, reels_query_hash, &reels_vars).await {
            posts.extend(self.parse_feed_items(&stories_body));
        }

        Ok(posts)
    }

    async fn fetch_profile(&mut self, credential: &Credential, username: &str) -> Result<SocialUser, String> {
        let mut client = HttpClient::with_session(&credential.session_token);

        let query_hash = "c9100b02e6f11f0823bcfc48906d57e9";
        let variables = serde_json::json!({
            "username": username,
            "fetch_highlight_reel": false,
            "fetch_mutual_followers": false,
        });

        let body = self.fetch_graphql(&mut client, query_hash, &variables).await?;

        let id = self.extract_user_id(&body);
        let username = self.extract_username(&body);
        let followers = self.extract_followers(&body);
        let following = self.extract_following(&body);

        Ok(SocialUser {
            id,
            platform: Platform::Instagram,
            username,
            follows: following,
            followers,
            last_sync: chrono::Utc::now().timestamp() as u64,
        })
    }

    async fn fetch_messages(&mut self, credential: &Credential) -> Result<Vec<Message>, String> {
        let mut client = HttpClient::with_session(&credential.session_token);
        let _csrf = self.extract_csrf(&mut client).await?;

        let url = "https://www.instagram.com/api/v1/direct_v2/inbox/?persist_relay=true";
        let body = client.get_json(url, Some("https://www.instagram.com/direct/inbox/")).await?;

        let mut msgs = Vec::new();
        if let Some(threads) = body["inbox"]["threads"].as_array() {
            for thread in threads {
                let conv_id = thread["thread_id"].as_str().unwrap_or("").to_string();
                if let Some(items) = thread["items"].as_array() {
                    for item in items {
                        let msg_id = item["item_id"].as_str().unwrap_or("").to_string();
                        let text = item["text"].as_str().unwrap_or("").to_string();
                        let sender = item["user_id"].as_i64().map(|i| i.to_string()).unwrap_or_default();
                        let ts = item["timestamp"].as_i64().unwrap_or(0) as u64 / 1000;

                        msgs.push(Message {
                            id: msg_id,
                            platform: Platform::Instagram,
                            conversation_id: conv_id.clone(),
                            sender_id: sender,
                            content: text,
                            timestamp: ts,
                        });
                    }
                }
            }
        }

        Ok(msgs)
    }

    async fn refresh_session(&mut self, credential: &Credential) -> Result<Credential, String> {
        let mut client = HttpClient::with_session(&credential.session_token);
        let _csrf = self.extract_csrf(&mut client).await?;

        let resp = client.client()
            .get("https://www.instagram.com/api/v1/accounts/current_user/")
            .header("User-Agent", self.user_agent())
            .send()
            .await
            .map_err(|e| format!("session refresh error: {}", e))?;

        let set_cookies = resp.headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|v| v.to_str().ok())
            .collect::<Vec<_>>()
            .join("; ");

        if set_cookies.contains("sessionid=") {
            let updated = format!("{}; {}", credential.session_token, set_cookies);
            return Ok(Credential {
                session_token: updated,
                ..credential.clone()
            });
        }

        Ok(credential.clone())
    }
}

fn urlencoding(s: &str) -> String {
    s.chars().map(|c| match c {
        'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        ' ' => "%20".into(),
        '=' => "%3D".into(),
        '&' => "%26".into(),
        '{' => "%7B".into(),
        '}' => "%7D".into(),
        '"' => "%22".into(),
        ':' => "%3A".into(),
        ',' => "%2C".into(),
        '[' => "%5B".into(),
        ']' => "%5D".into(),
        _ => format!("%{:02X}", c as u8),
    }).collect()
}
