use async_trait::async_trait;
use crate::http::HttpClient;
use crate::types::{Credential, Message, Platform, Post, SocialUser, StoryItem, StoryUser};
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

    #[allow(dead_code)] // legacy parser; superseded by post_from_media
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
                            poster_url: node.get("display_url").and_then(|v| v.as_str()).map(String::from),
                            liker_ids: likers,
                            commenter_ids: commenters,
                            timestamp,
                            is_video,
                            author_is_mutual: None,
                            author_is_close_friend: None,
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

    #[allow(dead_code)] // legacy parser; superseded by post_from_media
    fn parse_item(&self, item: &serde_json::Value) -> Option<Post> {
        let id = item["id"].as_str()?.to_string();
        let text = item["caption"]["text"].as_str().unwrap_or("").to_string();
        let code = item["code"].as_str().unwrap_or(&id);

        let author_id = item["user"]["pk"].as_i64().map(|i| i.to_string()).unwrap_or_default();
        let author_username = item["user"]["username"].as_str().unwrap_or("").to_string();

        let (is_video, media_urls) = classify_media(item);
        let (mutual, close_friend) = friendship_flags(&item["user"]);

        let timestamp = item["taken_at"].as_i64().unwrap_or(0) as u64;

        Some(Post {
            id: code.to_string(),
            platform: Platform::Instagram,
            author_id,
            author_username,
            content: text,
            media_urls,
            poster_url: extract_poster_url(item),
            liker_ids: Vec::new(),
            commenter_ids: Vec::new(),
            timestamp,
            is_video,
            author_is_mutual: mutual,
            author_is_close_friend: close_friend,
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

    pub async fn search_posts(&mut self, credential: &Credential, query: &str) -> Result<Vec<Post>, String> {
        let token = ensure_sessionid_prefix(&credential.session_token);
        let client = HttpClient::with_session(&token);

        let encoded = urlencoding(query);
        let url = format!("https://www.instagram.com/web/search/topsearch/?query={}", encoded);
        let resp = client.client()
            .get(&url)
            .header("X-IG-App-ID", "936619743392459")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", "https://www.instagram.com/explore/search/")
            .send()
            .await
            .map_err(|e| format!("http: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("body: {}", e))?;
        if !status.is_success() {
            return Err(format!("HTTP {}: {}", status.as_u16(), text.chars().take(200).collect::<String>()));
        }
        let body: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("json: {} (body: {})", e, text.chars().take(200).collect::<String>()))?;

        let mut users: Vec<(String, String)> = Vec::new();
        if let Some(arr) = body["users"].as_array() {
            for u in arr {
                let user = &u["user"];
                let pk = user["pk"].as_str()
                    .map(String::from)
                    .or_else(|| user["pk"].as_i64().map(|i| i.to_string()))
                    .unwrap_or_default();
                let username = user["username"].as_str().unwrap_or("").to_string();
                if !pk.is_empty() {
                    users.push((pk, username));
                }
            }
        }

        let mut hashtags: Vec<String> = Vec::new();
        if let Some(arr) = body["hashtags"].as_array() {
            for t in arr {
                if let Some(name) = t["hashtag"]["name"].as_str() {
                    hashtags.push(name.to_string());
                }
            }
        }

        let mut posts = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for (pk, username) in users {
            if posts.len() >= 30 { break; }
            let media_url = format!("https://www.instagram.com/api/v1/feed/user/{}/?count=12", pk);
            if let Ok(resp) = client.client()
                .get(&media_url)
                .header("X-IG-App-ID", "936619743392459")
                .header("X-Requested-With", "XMLHttpRequest")
                .header("Referer", &format!("https://www.instagram.com/{}/", username))
                .send()
                .await
            {
                if let Ok(t) = resp.text().await {
                    if let Ok(user_body) = serde_json::from_str::<serde_json::Value>(&t) {
                        let items = user_body["items"].as_array().or(user_body["feed_items"].as_array());
                        if let Some(items) = items {
                            for item in items {
                                let m = item.get("media_or_ad").unwrap_or(item);
                                if let Some(mut post) = post_from_media(m) {
                                    if post.author_id.is_empty() { post.author_id = pk.clone(); }
                                    if post.author_username.is_empty() { post.author_username = username.clone(); }
                                    if seen_ids.insert(post.id.clone()) {
                                        posts.push(post);
                                    }
                                }
                                if posts.len() >= 30 { break; }
                            }
                        }
                    }
                }
            }
        }

        for tag in &hashtags {
            if posts.len() >= 30 { break; }
            let tag_url = format!("https://www.instagram.com/api/v1/tags/{}/sections/", tag);
            let payload = serde_json::json!({
                "surface": "grid",
                "tab": "recent",
                "page_type": "tags",
                "include_persistent": true,
            });
            let csrf = self.extract_csrf(&client).await.unwrap_or_default();
            if let Ok(resp) = client.client()
                .post(&tag_url)
                .json(&payload)
                .header("X-IG-App-ID", "936619743392459")
                .header("X-Requested-With", "XMLHttpRequest")
                .header("Content-Type", "application/json")
                .header("Referer", "https://www.instagram.com/explore/tags/")
                .header("X-CSRFToken", &csrf)
                .send()
                .await
            {
                if let Ok(t) = resp.text().await {
                    if let Ok(tag_body) = serde_json::from_str::<serde_json::Value>(&t) {
                        for section in tag_body["sections"].as_array().unwrap_or(&vec![]) {
                            for item in section["layout_content"]["medias"].as_array().unwrap_or(&vec![]) {
                                if let Some(m) = item.get("media") {
                                    if let Some(post) = post_from_media(m) {
                                        if seen_ids.insert(post.id.clone()) {
                                            posts.push(post);
                                        }
                                    }
                                    if posts.len() >= 30 { break; }
                                }
                            }
                            if posts.len() >= 30 { break; }
                        }
                    }
                }
            }
        }

        Ok(posts)
    }

    pub async fn fetch_stories(&mut self, credential: &Credential) -> Result<Vec<StoryUser>, String> {
        let token = ensure_sessionid_prefix(&credential.session_token);
        let client = HttpClient::with_session(&token);

        let url = "https://www.instagram.com/api/v1/feed/reels_tray/";
        let resp = client.client()
            .get(url)
            .header("X-IG-App-ID", "936619743392459")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", "https://www.instagram.com/")
            .send()
            .await
            .map_err(|e| format!("http: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("body: {}", e))?;
        if !status.is_success() {
            return Err(format!("HTTP {}: {} (url: {})", status.as_u16(), text.chars().take(200).collect::<String>(), url));
        }
        let body: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("json: {}", e))?;

        let users = parse_stories(&body);
        if users.is_empty() {
            return Ok(users);
        }

        let mut users = users;
        users.truncate(30);
        let ids: Vec<String> = users.iter().map(|u| u.id.clone()).collect();
        let url = format!(
            "https://www.instagram.com/api/v1/feed/reels_media/?reel_ids={}",
            ids.join(",")
        );
        let resp = client.client()
            .get(&url)
            .header("X-IG-App-ID", "936619743392459")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", "https://www.instagram.com/")
            .send()
            .await
            .map_err(|e| format!("http: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("body: {}", e))?;
        if !status.is_success() {
            return Err(format!("HTTP {}: {} (url: {})", status.as_u16(), text.chars().take(200).collect::<String>(), url));
        }
        let body: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("json: {}", e))?;

        let items_by_id = parse_reels_media_batch(&body);
        for user in &mut users {
            if let Some(items) = items_by_id.get(&user.id) {
                user.items = items.clone();
            }
        }
        users.retain(|u| !u.items.is_empty());
        users.sort_by(|a, b| b.items.len().cmp(&a.items.len()));
        Ok(users)
    }

    pub async fn fetch_comments(&mut self, credential: &Credential, media_id: &str) -> Result<Vec<crate::types::Comment>, String> {
        let token = ensure_sessionid_prefix(&credential.session_token);
        let client = HttpClient::with_session(&token);

        let shortcode = if media_id.contains('_') {
            media_id_to_shortcode(media_id)
                .ok_or_else(|| format!("cannot derive shortcode from media id {}", media_id))?
        } else {
            media_id.to_string()
        };

        let vars = format!(
            "{{\"shortcode\":\"{}\",\"first\":50}}",
            shortcode
        );
        let url = format!(
            "https://www.instagram.com/graphql/query/?query_hash=97b41c52301f77ce508f55e66d17620e&variables={}",
            urlencoding(&vars)
        );
        let resp = client.client()
            .get(&url)
            .header("X-IG-App-ID", "936619743392459")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", &format!("https://www.instagram.com/p/{}/", shortcode))
            .send()
            .await
            .map_err(|e| format!("http: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("body: {}", e))?;
        if !status.is_success() {
            return Err(format!("HTTP {}: {} (url: {})", status.as_u16(), text.chars().take(200).collect::<String>(), url));
        }
        let body: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("json: {} (body: {})", e, text.chars().take(200).collect::<String>()))?;

        let mut comments = parse_comments(&body);
        for c in &mut comments {
            c.post_id = media_id.to_string();
            c.is_mine = !credential.user_id.is_empty() && c.author_id == credential.user_id;
        }
        Ok(comments)
    }

    async fn fetch_graphql(&self, client: &HttpClient, query_hash: &str, variables: &serde_json::Value) -> Result<serde_json::Value, String> {
        let vars = serde_json::to_string(variables).map_err(|e| e.to_string())?;
        let url = format!(
            "https://www.instagram.com/graphql/query/?query_hash={}&variables={}",
            query_hash, urlencoding(&vars)
        );
        client.get_json(&url, Some("https://www.instagram.com/")).await
    }

    pub async fn fetch_inbox(&mut self, credential: &Credential) -> Result<Vec<(crate::types::Conversation, Vec<Message>)>, String> {
        let token = ensure_sessionid_prefix(&credential.session_token);
        let client = HttpClient::with_session(&token);
        let _csrf = self.extract_csrf(&client).await.unwrap_or_default();

        let url = "https://www.instagram.com/api/v1/direct_v2/inbox/?persist_relay=true";
        let resp = client.client()
            .get(url)
            .header("X-IG-App-ID", "936619743392459")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", "https://www.instagram.com/direct/inbox/")
            .send()
            .await
            .map_err(|e| format!("http: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("body: {}", e))?;
        if !status.is_success() {
            return Err(format!("HTTP {}: {} (url: {})", status.as_u16(), text.chars().take(200).collect::<String>(), url));
        }
        let body: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("json: {} (body: {})", e, text.chars().take(200).collect::<String>()))?;

        Ok(parse_threads(&body, &credential.user_id))
    }
}

#[async_trait]
impl PlatformIngester for InstagramIngester {
    fn platform(&self) -> Platform {
        Platform::Instagram
    }

    async fn fetch_feed(&mut self, credential: &Credential) -> Result<Vec<Post>, String> {
        let token = ensure_sessionid_prefix(&credential.session_token);
        let client = HttpClient::with_session(&token);

        let csrf = self.extract_csrf(&client).await.unwrap_or_default();
        let url = "https://www.instagram.com/api/v1/feed/timeline/?count=12";

        let _ = &client;
        let _ = &csrf;
        let resp = client.client()
            .get(url)
            .header("X-IG-App-ID", "936619743392459")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", "https://www.instagram.com/")
            .send()
            .await
            .map_err(|e| format!("http: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("body: {}", e))?;
        if !status.is_success() {
            return Err(format!("HTTP {}: {} (url: {})", status.as_u16(), text.chars().take(200).collect::<String>(), url));
        }
        let body: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("json: {} (body: {})", e, text.chars().take(200).collect::<String>()))?;

        let mut posts = Vec::new();
        if let Some(feed_items) = body["feed_items"].as_array().or(body["items"].as_array()) {
            for item in feed_items {
                let media = item.get("media_or_ad")
                    .or_else(|| item.get("media"))
                    .or_else(|| item.get("image"))
                    .or_else(|| item.get("video"));
                let m = match media {
                    Some(m) => m,
                    None => {
                        if let Some(m) = item.get("carousel_media") {
                            m
                        } else {
                            continue;
                        }
                    }
                };

                let id = m["id"].as_str().or(item["id"].as_str()).unwrap_or("").to_string();
                if id.is_empty() { continue; }

                let _code = m["code"].as_str().or(item["code"].as_str()).unwrap_or("").to_string();
                let text = m["caption"]["text"].as_str()
                    .or_else(|| item["caption"]["text"].as_str())
                    .unwrap_or("")
                    .to_string();
                let user = m["user"].as_object().or_else(|| item["user"].as_object());
                let author_id = user.and_then(|u| u["pk"].as_i64().or(u["id"].as_i64())).map(|i| i.to_string()).unwrap_or_default();
                let author_username = user.and_then(|u| u["username"].as_str()).unwrap_or("").to_string();
                let ts = m["taken_at"].as_i64().or(item["taken_at"].as_i64()).unwrap_or(0) as u64;
                let (is_video, media_urls) = classify_media(&m);
                let (mutual, close_friend) = friendship_flags(&m);

                let likers: Vec<String> = m["like_count"].as_i64().map(|_| vec![]).unwrap_or_default();
                let commenters: Vec<String> = m["comment_count"].as_i64().map(|_| vec![]).unwrap_or_default();
                let likes = m["like_count"].as_i64().unwrap_or(0) as u32;
                let comments = m["comment_count"].as_i64().unwrap_or(0) as u32;
                let engagement = if likes + comments > 0 { Some((likes + comments) as f32) } else { None };

                posts.push(Post {
                    id, platform: Platform::Instagram,
                    author_id, author_username,
                    content: text,
                    media_urls, poster_url: extract_poster_url(&m), liker_ids: likers, commenter_ids: commenters,
                    timestamp: ts, is_video,
                    author_is_mutual: mutual,
                    author_is_close_friend: close_friend,
                    engagement_score: engagement,
                    is_synthetic: None,
                    vector_embedding: None,
                });
            }
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
        let token = ensure_sessionid_prefix(&credential.session_token);
        let client = HttpClient::with_session(&token);
        let _csrf = self.extract_csrf(&client).await.unwrap_or_default();

        let url = "https://www.instagram.com/api/v1/direct_v2/inbox/?persist_relay=true";
        let resp = client.client()
            .get(url)
            .header("X-IG-App-ID", "936619743392459")
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", "https://www.instagram.com/direct/inbox/")
            .send()
            .await
            .map_err(|e| format!("http: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("body: {}", e))?;
        if !status.is_success() {
            return Err(format!("HTTP {}: {} (url: {})", status.as_u16(), text.chars().take(200).collect::<String>(), url));
        }
        let body: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("json: {} (body: {})", e, text.chars().take(200).collect::<String>()))?;

        Ok(parse_direct_body(&body))
    }

    async fn send_message(&mut self, credential: &Credential, thread_id: &str, content: &str) -> Result<(), String> {
        let token = ensure_sessionid_prefix(&credential.session_token);
        let client = HttpClient::with_session(&token);
        let csrf = self.extract_csrf(&client).await?;
        let ctx = random_hex_id();
        let offline = random_hex_id();
        let url = format!("https://www.instagram.com/api/v1/direct_v2/threads/{}/items/", thread_id);
        let resp = client
            .client()
            .post(&url)
            .header("X-IG-App-ID", "936619743392459")
            .header("X-CSRFToken", csrf.as_str())
            .header("X-Requested-With", "XMLHttpRequest")
            .header("Referer", &format!("https://www.instagram.com/direct/t/{}/", thread_id))
            .form(&[
                ("client_context", ctx.as_str()),
                ("action", "send_item"),
                ("item_type", "text"),
                ("text", content.trim()),
                ("mutation_token", ctx.as_str()),
                ("offline_threading_id", offline.as_str()),
            ])
            .send()
            .await
            .map_err(|e| format!("send dm http: {}", e))?;
        let status = resp.status();
        let text = resp.text().await.map_err(|e| format!("send body: {}", e))?;
        if !status.is_success() {
            return Err(format!("Instagram rejected send (HTTP {}): {}", status.as_u16(), text.chars().take(160).collect::<String>()));
        }
        Ok(())
    }

    async fn fetch_inbox(&mut self, credential: &Credential) -> Result<Vec<(crate::types::Conversation, Vec<Message>)>, String> {
        self.fetch_inbox(credential).await
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

pub fn ensure_sessionid_prefix(token: &str) -> String {
    if token.starts_with("sessionid=") {
        token.to_string()
    } else {
        format!("sessionid={}", token)
    }
}

/// A random hex string used for IG direct-message client/mutation ids.
fn random_hex_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let mut s = String::with_capacity(32);
    for _ in 0..32 {
        s.push_str(&format!("{:x}", rng.gen_range(0u32..16)));
    }
    s
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

pub fn classify_media(m: &serde_json::Value) -> (bool, Vec<String>) {
    let media = if m["media_type"].as_i64() == Some(8) {
        m["carousel_media"].as_array().and_then(|a| a.first()).unwrap_or(m)
    } else {
        m
    };
    let is_video = media["media_type"].as_i64().map_or(false, |t| t == 2 || t == 3)
        || media["is_video"].as_bool().unwrap_or(false)
        || media["video_versions"].as_array().map(|a| !a.is_empty()).unwrap_or(false);

    let mut media_urls = Vec::new();
    if is_video {
        if let Some(a) = media["video_versions"].as_array() {
            media_urls.extend(a.iter().filter_map(|v| v["url"].as_str().map(String::from)));
        }
        if media_urls.is_empty() {
            if let Some(src) = media["image_versions2"]["candidates"].as_array().and_then(|a| a.first()).and_then(|c| c["url"].as_str()) {
                media_urls.push(src.to_string());
            }
        }
    } else {
        if let Some(src) = media["image_versions2"]["candidates"].as_array().and_then(|a| a.first()).and_then(|c| c["url"].as_str()) {
            media_urls.push(src.to_string());
        }
    }
    (is_video, media_urls)
}

pub fn friendship_flags(media: &serde_json::Value) -> (Option<bool>, Option<bool>) {
    let fs = &media["user"]["friendship_status"];
    let followed_by = fs["followed_by"].as_bool().unwrap_or(false);
    let following = fs["following"].as_bool().unwrap_or(false);
    let is_bestie = fs["is_bestie"].as_bool().unwrap_or(false);
    (Some(followed_by && following), Some(is_bestie))
}

fn extract_poster_url(item: &serde_json::Value) -> Option<String> {
    item["image_versions2"]["candidates"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|c| c["url"].as_str().map(String::from))
}

fn parse_user_pk(user: &serde_json::Value) -> String {
    user["pk"]
        .as_str()
        .map(String::from)
        .or_else(|| user["pk"].as_i64().map(|i| i.to_string()))
        .or_else(|| user["id"].as_i64().map(|i| i.to_string()))
        .unwrap_or_default()
}

fn post_from_media(m: &serde_json::Value) -> Option<Post> {
    let id = m["id"].as_str()
        .map(String::from)
        .or_else(|| m["pk"].as_i64().map(|i| i.to_string()))?;
    let code = m["code"].as_str().unwrap_or(&id);
    let text = m["caption"]["text"].as_str().unwrap_or("").to_string();
    let author_id = parse_user_pk(&m["user"]);
    let author_username = m["user"]["username"].as_str().unwrap_or("").to_string();
    let ts = m["taken_at"].as_i64().unwrap_or(0) as u64;
    let (is_video, media_urls) = classify_media(m);
    let (mutual, close_friend) = friendship_flags(m);
    let likes = m["like_count"].as_i64().unwrap_or(0) as u32;
    let comments = m["comment_count"].as_i64().unwrap_or(0) as u32;
    let engagement = if likes + comments > 0 { Some((likes + comments) as f32) } else { None };

    Some(Post {
        id: code.to_string(),
        platform: Platform::Instagram,
        author_id,
        author_username,
        content: text,
        media_urls,
        poster_url: extract_poster_url(m),
        liker_ids: Vec::new(),
        commenter_ids: Vec::new(),
        timestamp: ts,
        is_video,
        author_is_mutual: mutual,
        author_is_close_friend: close_friend,
        engagement_score: engagement,
        is_synthetic: None,
        vector_embedding: None,
    })
}

const SHORTCODE_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

pub fn pk_to_shortcode(pk: &str) -> Option<String> {
    let mut num = pk.parse::<u128>().ok()?;
    if num == 0 {
        return None;
    }
    let mut result = String::new();
    while num > 0 {
        let idx = (num % 64) as usize;
        result.push(SHORTCODE_ALPHABET[idx] as char);
        num /= 64;
    }
    Some(result.chars().rev().collect())
}

pub fn media_id_to_shortcode(media_id: &str) -> Option<String> {
    let pk = media_id.split('_').next().unwrap_or(media_id);
    pk_to_shortcode(pk)
}

pub fn parse_stories(body: &serde_json::Value) -> Vec<StoryUser> {
    let mut stories = Vec::new();
    if let Some(tray) = body["tray"].as_array() {
        for entry in tray {
            let user = entry.get("user");
            if user.is_none() || user.unwrap().is_null() {
                continue;
            }
            let user = user.unwrap();
            let id = user["pk"].as_i64()
                .map(|i| i.to_string())
                .or_else(|| user["pk"].as_str().map(String::from))
                .unwrap_or_default();
            let username = user["username"].as_str().unwrap_or("").to_string();
            let profile_pic_url = user["profile_pic_url"].as_str().unwrap_or("").to_string();
            let fs = &user["friendship_status"];
            let is_mutual = fs["followed_by"].as_bool().unwrap_or(false) && fs["following"].as_bool().unwrap_or(false);
            let is_close_friend = fs["is_bestie"].as_bool().unwrap_or(false);

            let mut items = Vec::new();
            if let Some(item_arr) = entry["items"].as_array() {
                for item in item_arr {
                    if let Some(story) = parse_story_item(item) {
                        items.push(story);
                    }
                }
            }
            stories.push(StoryUser {
                id,
                username,
                profile_pic_url,
                is_mutual,
                is_close_friend,
                items,
            });
        }
    }
    stories
}

pub fn parse_reel_media(body: &serde_json::Value) -> Vec<StoryItem> {
    let mut items = Vec::new();
    for arr in [body["items"].as_array(), body["reel"]["items"].as_array()] {
        if let Some(arr) = arr {
            for item in arr {
                if let Some(story) = parse_story_item(item) {
                    items.push(story);
                }
            }
        }
    }
    items
}

pub fn parse_reels_media_batch(body: &serde_json::Value) -> std::collections::HashMap<String, Vec<StoryItem>> {
    let mut map = std::collections::HashMap::new();
    if let Some(reels) = body["reels"].as_object() {
        for (user_id, reel) in reels {
            let items = parse_reel_media(reel);
            if !items.is_empty() {
                map.insert(user_id.clone(), items);
            }
        }
    }
    map
}

pub fn parse_comments(body: &serde_json::Value) -> Vec<crate::types::Comment> {
    let mut comments = Vec::new();

    if let Some(edges) = body["data"]["shortcode_media"]["edge_media_to_parent_comment"]["edges"].as_array() {
        for edge in edges {
            let node = &edge["node"];
            let id = node["id"].as_str().unwrap_or("").to_string();
            if id.is_empty() {
                continue;
            }
            let author_id = node["owner"]["id"].as_str().unwrap_or("").to_string();
            let author_username = node["owner"]["username"].as_str().unwrap_or("").to_string();
            let content = node["text"].as_str().unwrap_or("").to_string();
            let timestamp = node["created_at"].as_i64().unwrap_or(0) as u64;
            let likes = node["edge_liked_by"]["count"].as_u64().unwrap_or(0) as u32;

            comments.push(crate::types::Comment {
                id,
                post_id: String::new(),
                platform: crate::types::Platform::Instagram,
                author_id,
                author_username,
                content,
                timestamp,
                likes,
                is_mine: false,
            });
        }
        return comments;
    }

    if let Some(arr) = body["comments"].as_array() {
        for c in arr {
            let id = c["pk"].as_str().unwrap_or("").to_string();
            if id.is_empty() {
                continue;
            }
            let author_id = c["user"]["pk"].as_i64().map(|i| i.to_string())
                .or_else(|| c["user"]["pk"].as_str().map(String::from))
                .unwrap_or_default();
            let author_username = c["user"]["username"].as_str().unwrap_or("").to_string();
            let content = c["text"].as_str().unwrap_or("").to_string();
            let timestamp = c["created_at"].as_i64().unwrap_or(0) as u64;
            let likes = c["comment_like_count"].as_u64().unwrap_or(0) as u32;

            comments.push(crate::types::Comment {
                id,
                post_id: String::new(),
                platform: crate::types::Platform::Instagram,
                author_id,
                author_username,
                content,
                timestamp,
                likes,
                is_mine: false,
            });
        }
    }
    comments
}

pub fn parse_direct_body(body: &serde_json::Value) -> Vec<Message> {
    let mut msgs = Vec::new();
    if let Some(threads) = body["inbox"]["threads"].as_array() {
        for thread in threads {
            let conv_id = thread["thread_id"].as_str().unwrap_or("").to_string();
            if conv_id.is_empty() {
                continue;
            }
            if let Some(items) = thread["items"].as_array() {
                for item in items {
                    if item["item_type"].as_str().map(|t| t != "text").unwrap_or(false) {
                        continue;
                    }
                    let text = item["text"].as_str().unwrap_or("").trim().to_string();
                    if text.is_empty() {
                        continue;
                    }
                    let msg_id = item["item_id"].as_str().unwrap_or("").to_string();
                    let sender = item["user_id"].as_i64()
                        .map(|i| i.to_string())
                        .or_else(|| item["user_id"].as_str().map(String::from))
                        .unwrap_or_default();
                    let ts_raw = item["timestamp"].as_i64().unwrap_or(0) as u64;
                    let ts = if ts_raw > 1_000_000_000_000 { ts_raw / 1_000_000 } else { ts_raw };

                    msgs.push(Message {
                        id: msg_id,
                        platform: crate::types::Platform::Instagram,
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

fn parse_pk(v: &serde_json::Value) -> String {
    v.as_i64()
        .map(|i| i.to_string())
        .or_else(|| v.as_str().map(String::from))
        .unwrap_or_default()
}

pub fn parse_threads(body: &serde_json::Value, viewer_pk: &str) -> Vec<(crate::types::Conversation, Vec<Message>)> {
    let mut result = Vec::new();
    if let Some(threads) = body["inbox"]["threads"].as_array() {
        for thread in threads {
            let conv_id = thread["thread_id"].as_str().unwrap_or("").to_string();
            if conv_id.is_empty() {
                continue;
            }

            let mut name_by_pk = std::collections::HashMap::new();
            if let Some(users) = thread["users"].as_array() {
                for u in users {
                    let pk = parse_pk(&u["pk"]);
                    let name = u["username"].as_str().unwrap_or("").to_string();
                    if !name.is_empty() {
                        name_by_pk.insert(pk, name);
                    }
                }
            }

            let mut participants: Vec<String> = name_by_pk.iter()
                .filter(|(pk, _)| pk.as_str() != viewer_pk)
                .map(|(_, name)| name.clone())
                .collect();
            participants.sort();
            if participants.is_empty() {
                if let Some(title) = thread["thread_title"].as_str() {
                    if !title.is_empty() {
                        participants.push(title.to_string());
                    }
                }
            }
            if participants.is_empty() {
                participants.push("Unknown".into());
            }

            let mut msgs = Vec::new();
            let mut last_ts = 0u64;
            if let Some(items) = thread["items"].as_array() {
                for item in items {
                    let item_type = item["item_type"].as_str().unwrap_or("");
                    let is_text = item_type == "text";
                    if !is_text && item_type != "media" && item_type != "voice_media" && item_type != "link" && item_type != "raven_media" {
                        continue;
                    }
                    let msg_id = item["item_id"].as_str().unwrap_or("").to_string();
                    if msg_id.is_empty() {
                        continue;
                    }
                    let pk = parse_pk(&item["user_id"]);
                    let sender = if pk == viewer_pk {
                        "You".to_string()
                    } else {
                        name_by_pk.get(&pk).cloned().unwrap_or_else(|| pk.clone())
                    };
                    let ts_raw = item["timestamp"].as_i64().unwrap_or(0) as u64;
                    let ts = if ts_raw > 1_000_000_000_000 { ts_raw / 1_000_000 } else { ts_raw };
                    let content = if is_text {
                        item["text"].as_str().unwrap_or("").trim().to_string()
                    } else {
                        format!("[{}]", item_type)
                    };
                    if is_text && content.is_empty() {
                        continue;
                    }
                    last_ts = last_ts.max(ts);

                    msgs.push(Message {
                        id: msg_id,
                        platform: crate::types::Platform::Instagram,
                        conversation_id: conv_id.clone(),
                        sender_id: sender,
                        content,
                        timestamp: ts,
                        is_mine: pk == viewer_pk,
                    });
                }
            }

            result.push((
                crate::types::Conversation {
                    id: conv_id.clone(),
                    platform: crate::types::Platform::Instagram,
                    participants,
                    last_message_at: last_ts,
                    unread: thread["has_newer"].as_bool().unwrap_or(false),
                },
                msgs,
            ));
        }
    }
    result.sort_by(|a, b| b.0.last_message_at.cmp(&a.0.last_message_at));
    result
}


pub fn parse_story_item(item: &serde_json::Value) -> Option<StoryItem> {
    let id = item["id"].as_str()?.to_string();
    let media_type = item["media_type"].as_i64().unwrap_or(0) as u8;
    let is_video = media_type == 2;

    let mut media_url = String::new();
    let mut poster_url = None;
    if is_video {
        if let Some(src) = item["video_versions"].as_array().and_then(|a| a.first()).and_then(|v| v["url"].as_str()) {
            media_url = src.to_string();
        }
        if let Some(src) = item["image_versions2"]["candidates"].as_array().and_then(|a| a.last()).and_then(|c| c["url"].as_str()) {
            poster_url = Some(src.to_string());
        }
    } else {
        if let Some(src) = item["image_versions2"]["candidates"].as_array().and_then(|a| a.first()).and_then(|c| c["url"].as_str()) {
            media_url = src.to_string();
        }
    }

    let timestamp = item["taken_at"].as_i64().unwrap_or(0) as u64;
    let expiring_at = item["expiring_at"].as_i64().unwrap_or(0) as u64;
    let caption = item["caption"].as_str().unwrap_or("").to_string();

    Some(StoryItem {
        id,
        media_type,
        media_url,
        poster_url,
        is_video,
        timestamp,
        expiring_at,
        caption,
    })
}

#[cfg(test)]
mod story_tests {
    use super::*;

    fn photo_item() -> serde_json::Value {
        serde_json::json!({
            "id": "story1",
            "media_type": 1,
            "image_versions2": { "candidates": [ { "url": "https://cdn.example.com/photo.jpg", "width": 640, "height": 1136 } ] },
            "taken_at": 1700000000,
            "expiring_at": 1700086400,
            "caption": null
        })
    }

    fn video_item() -> serde_json::Value {
        serde_json::json!({
            "id": "story2",
            "media_type": 2,
            "video_versions": [ { "url": "https://cdn.example.com/clip.mp4", "width": 720, "height": 1280 } ],
            "image_versions2": { "candidates": [ { "url": "https://cdn.example.com/poster.jpg", "width": 1080, "height": 1920 } ] },
            "taken_at": 1700000001,
            "expiring_at": 1700086401,
            "caption": "today"
        })
    }

    #[test]
    fn test_parse_photo_story() {
        let item = parse_story_item(&photo_item()).expect("should parse");
        assert_eq!(item.id, "story1");
        assert!(!item.is_video);
        assert_eq!(item.media_url, "https://cdn.example.com/photo.jpg");
        assert_eq!(item.timestamp, 1700000000);
        assert!(item.poster_url.is_none());
    }

    #[test]
    fn test_parse_video_story() {
        let item = parse_story_item(&video_item()).expect("should parse");
        assert!(item.is_video);
        assert_eq!(item.media_url, "https://cdn.example.com/clip.mp4");
        assert_eq!(item.poster_url.as_deref(), Some("https://cdn.example.com/poster.jpg"));
        assert_eq!(item.caption, "today");
    }

    #[test]
    fn test_parse_story_missing_id() {
        assert!(parse_story_item(&serde_json::json!({ "media_type": 1 })).is_none());
    }

    #[test]
    fn test_fetch_stories_parses_tray() {
        let body = serde_json::json!({
            "tray": [
                {
                    "user": { "pk": 123, "username": "alice", "profile_pic_url": "https://cdn.example.com/ava.jpg",
                              "friendship_status": { "followed_by": true, "following": true, "is_bestie": true } },
                    "items": [ photo_item() ]
                },
                {
                    "user": { "pk": 456, "username": "bob", "profile_pic_url": "",
                              "friendship_status": { "followed_by": false, "following": true, "is_bestie": false } },
                    "items": [ video_item() ]
                }
            ]
        });

        let stories = parse_stories(&body);

        assert_eq!(stories.len(), 2);
        assert_eq!(stories[0].username, "alice");
        assert_eq!(stories[0].profile_pic_url, "https://cdn.example.com/ava.jpg");
        assert_eq!(stories[0].items[0].id, "story1");
        assert!(stories[0].is_mutual);
        assert!(stories[0].is_close_friend);
        assert_eq!(stories[1].username, "bob");
        assert_eq!(stories[1].items[0].media_url, "https://cdn.example.com/clip.mp4");
        assert_eq!(stories[1].items[0].is_video, true);
        assert!(!stories[1].is_mutual);
        assert!(!stories[1].is_close_friend);
    }

    #[test]
    fn test_parse_stories_empty_tray() {
        assert!(parse_stories(&serde_json::json!({})).is_empty());
        assert!(parse_stories(&serde_json::json!({ "tray": [] })).is_empty());
    }

    #[test]
    fn test_parse_reel_media_top_level() {
        let body = serde_json::json!({
            "items": [ photo_item(), video_item() ]
        });
        let items = parse_reel_media(&body);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].id, "story1");
        assert_eq!(items[1].id, "story2");
    }

    #[test]
    fn test_parse_reel_media_nested_reel() {
        let body = serde_json::json!({
            "reel": { "items": [ video_item() ] }
        });
        let items = parse_reel_media(&body);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, "story2");
    }

    #[test]
    fn test_parse_reels_media_batch() {
        let body = serde_json::json!({
            "reels": {
                "123": { "items": [ photo_item() ] },
                "456": { "items": [ video_item() ] },
                "789": { "items": [] }
            }
        });
        let map = parse_reels_media_batch(&body);
        assert_eq!(map.len(), 2);
        assert_eq!(map["123"][0].id, "story1");
        assert_eq!(map["456"][0].id, "story2");
        assert!(!map.contains_key("789"));
    }
}

#[cfg(test)]
mod media_tests {
    use super::*;

    fn video_media() -> serde_json::Value {
        serde_json::json!({
            "id": "v1",
            "media_type": 2,
            "video_versions": [ { "url": "https://cdn.example.com/reel.mp4" }, { "url": "https://cdn.example.com/reel-low.mp4" } ],
            "image_versions2": { "candidates": [ { "url": "https://cdn.example.com/poster.jpg" } ] },
            "user": { "pk": 11, "username": "tester" }
        })
    }

    #[test]
    fn test_media_type_2_is_video() {
        let (is_video, urls) = classify_media(&video_media());
        assert!(is_video);
        assert_eq!(urls, vec!["https://cdn.example.com/reel.mp4", "https://cdn.example.com/reel-low.mp4"]);
    }

    #[test]
    fn test_media_type_1_is_image() {
        let m = serde_json::json!({
            "id": "i1", "media_type": 1,
            "image_versions2": { "candidates": [ { "url": "https://cdn.example.com/pic.jpg" } ] }
        });
        let (is_video, urls) = classify_media(&m);
        assert!(!is_video);
        assert_eq!(urls, vec!["https://cdn.example.com/pic.jpg"]);
    }

    #[test]
    fn test_carousel_uses_first_item() {
        let m = serde_json::json!({
            "id": "c1", "media_type": 8,
            "carousel_media": [ video_media(), serde_json::json!({ "media_type": 1 }) ]
        });
        let (is_video, urls) = classify_media(&m);
        assert!(is_video);
        assert_eq!(urls[0], "https://cdn.example.com/reel.mp4");
    }

    #[test]
    fn test_video_fallback_to_image() {
        let m = serde_json::json!({ "id": "v2", "media_type": 2, "is_video": true });
        let (is_video, urls) = classify_media(&m);
        assert!(is_video);
        assert!(urls.is_empty());
    }

    #[test]
    fn test_friendship_flags_mutual_and_bestie() {
        let m = serde_json::json!({
            "user": { "friendship_status": { "followed_by": true, "following": true, "is_bestie": true } }
        });
        let (mutual, close) = friendship_flags(&m);
        assert_eq!(mutual, Some(true));
        assert_eq!(close, Some(true));
    }

    #[test]
    fn test_friendship_flags_non_mutual() {
        let m = serde_json::json!({
            "user": { "friendship_status": { "followed_by": false, "following": true, "is_bestie": false } }
        });
        let (mutual, close) = friendship_flags(&m);
        assert_eq!(mutual, Some(false));
        assert_eq!(close, Some(false));
    }

    #[test]
    fn test_friendship_flags_missing() {
        let (mutual, close) = friendship_flags(&serde_json::json!({}));
        assert_eq!(mutual, Some(false));
        assert_eq!(close, Some(false));
    }

    #[test]
    fn test_parse_comments() {
        let body = serde_json::json!({
            "comments": [
                {
                    "pk": "17800000000000001",
                    "text": "nice shot",
                    "created_at": 1700000000,
                    "comment_like_count": 3,
                    "user": { "pk": 999, "username": "commenter1" }
                },
                {
                    "pk": "17800000000000002",
                    "text": "wow",
                    "user": { "pk": "888", "username": "commenter2" }
                }
            ]
        });
        let comments = parse_comments(&body);
        assert_eq!(comments.len(), 2);
        assert_eq!(comments[0].author_username, "commenter1");
        assert_eq!(comments[0].content, "nice shot");
        assert_eq!(comments[0].likes, 3);
        assert_eq!(comments[0].timestamp, 1700000000);
        assert_eq!(comments[1].author_id, "888");
    }

    #[test]
    fn test_parse_comments_skips_no_id() {
        let body = serde_json::json!({ "comments": [ { "text": "x" } ] });
        assert!(parse_comments(&body).is_empty());
    }

    #[test]
    fn test_pk_to_shortcode() {
        assert_eq!(pk_to_shortcode("3946311266234209654").as_deref(), Some("DbEHTAtAxV2"));
        assert_eq!(pk_to_shortcode("0"), None);
    }

    #[test]
    fn test_media_id_to_shortcode_strips_user_suffix() {
        assert_eq!(
            media_id_to_shortcode("3946311266234209654_40243796564").as_deref(),
            Some("DbEHTAtAxV2")
        );
    }

    #[test]
    fn test_parse_comments_graphql() {
        let body = serde_json::json!({
            "data": {
                "shortcode_media": {
                    "edge_media_to_parent_comment": {
                        "count": 45,
                        "edges": [
                            {
                                "node": {
                                    "id": "18109813249791225",
                                    "text": "great post",
                                    "created_at": 1700000000,
                                    "owner": { "id": "123", "username": "poster" },
                                    "edge_liked_by": { "count": 7 }
                                }
                            }
                        ]
                    }
                }
            }
        });
        let comments = parse_comments(&body);
        assert_eq!(comments.len(), 1);
        assert_eq!(comments[0].id, "18109813249791225");
        assert_eq!(comments[0].content, "great post");
        assert_eq!(comments[0].author_username, "poster");
        assert_eq!(comments[0].likes, 7);
        assert_eq!(comments[0].timestamp, 1700000000);
    }
}

#[cfg(test)]
mod dm_tests {
    use super::*;

    #[test]
    fn test_parse_direct_body_us_timestamp_string_sender() {
        let body = serde_json::json!({
            "inbox": {
                "threads": [
                    {
                        "thread_id": "conv1",
                        "items": [
                            {
                                "item_id": "m1",
                                "item_type": "text",
                                "text": "hello there",
                                "user_id": "12345",
                                "timestamp": 1785712616642883i64
                            },
                            {
                                "item_id": "m2",
                                "item_type": "action_log",
                                "timestamp": 1785712616000000i64
                            }
                        ]
                    }
                ]
            }
        });
        let msgs = parse_direct_body(&body);
        assert_eq!(msgs.len(), 1, "action_log items should be skipped");
        assert_eq!(msgs[0].id, "m1");
        assert_eq!(msgs[0].sender_id, "12345");
        assert_eq!(msgs[0].content, "hello there");
        assert_eq!(msgs[0].timestamp, 1785712616);
    }

    #[test]
    fn test_parse_direct_body_seconds_timestamp() {
        let body = serde_json::json!({
            "inbox": { "threads": [ { "thread_id": "c", "items": [
                { "item_id": "m", "item_type": "text", "text": "old", "user_id": 7, "timestamp": 1700000000 }
            ] } ] }
        });
        let msgs = parse_direct_body(&body);
        assert_eq!(msgs[0].timestamp, 1700000000);
    }

    #[test]
    fn test_parse_threads_usernames_and_you() {
        let body = serde_json::json!({
            "inbox": {
                "threads": [
                    {
                        "thread_id": "conv1",
                        "has_newer": true,
                        "users": [
                            { "pk": "999", "username": "alice" },
                            { "pk": "1000", "username": "bob" }
                        ],
                        "items": [
                            { "item_id": "m1", "item_type": "text", "text": "hi alice", "user_id": "1000", "timestamp": 1785712616642883i64 },
                            { "item_id": "m2", "item_type": "text", "text": "hey bob", "user_id": "999", "timestamp": 1785712617000000i64 }
                        ]
                    }
                ]
            }
        });
        let threads = parse_threads(&body, "999");
        assert_eq!(threads.len(), 1);
        let (conv, msgs) = &threads[0];
        assert!(conv.unread);
        assert_eq!(conv.participants, vec!["bob".to_string()]);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].sender_id, "bob");
        assert_eq!(msgs[1].sender_id, "You");
        assert_eq!(msgs[1].timestamp, 1785712617);
    }

    #[test]
    fn test_parse_threads_fallback_title() {
        let body = serde_json::json!({
            "inbox": { "threads": [ {
                "thread_id": "g1", "thread_title": "Family Group", "users": [], "items": []
            } ] }
        });
        let threads = parse_threads(&body, "999");
        assert_eq!(threads[0].0.participants, vec!["Family Group".to_string()]);
    }
}
