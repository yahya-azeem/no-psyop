use async_trait::async_trait;

use crate::http::HttpClient;
use crate::types::{Credential, Message, Platform, Post, SocialUser};
use super::PlatformIngester;

const API_BASE: &str = "https://www.linkedin.com/voyager/api";

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
        let mut client = HttpClient::with_session(&credential.session_token);
        let csrf = self.csrf_token(credential);
        client.set_cookies(&format!("JSESSIONID=\"{}\"", csrf));
        client
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

        let timestamp = (activity["temporal"]["time"].as_i64().unwrap_or(0)) as u64;

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
            liker_ids,
            commenter_ids: Vec::new(),
            timestamp,
            is_video,
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
                        let ts = (event["createdAt"].as_i64().unwrap_or(0)) as u64;

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
            "{}/feedDash/homeUpdates?moduleKey=feed&start=0&count=10&csrfToken={}",
            API_BASE,
            urlencoding::encode(&csrf)
        );

        let body = client.get_json(&url, Some("https://www.linkedin.com/feed/")).await?;
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

        let body = client.get_json(&url, Some("https://www.linkedin.com/in/")).await?;
        let mut profile = self.extract_profile(&body);

        if let Ok(followers_body) = client.get_json(&followers_url, Some("https://www.linkedin.com/in/")).await {
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

        let body = client.get_json(&url, Some("https://www.linkedin.com/messaging/")).await?;
        let msgs = self.extract_messages(&body);

        Ok(msgs)
    }

    async fn refresh_session(&mut self, credential: &Credential) -> Result<Credential, String> {
        let client = self.build_client(credential);
        let csrf = self.csrf_token(credential);

        let url = format!("{}/me?csrfToken={}", API_BASE, urlencoding::encode(&csrf));

        match client.get_json(&url, Some("https://www.linkedin.com/")).await {
            Ok(_) => Ok(credential.clone()),
            Err(e) => Err(format!("linkedin session expired: {}", e)),
        }
    }
}
