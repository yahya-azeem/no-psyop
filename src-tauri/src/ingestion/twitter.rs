use async_trait::async_trait;
use crate::http::HttpClient;
use crate::types::{Credential, Message, Platform, Post, SocialUser};
use super::PlatformIngester;

const API_BASE: &str = "https://api.twitter.com";
const GTG_BASE: &str = "https://x.com";

pub struct TwitterIngester;

impl TwitterIngester {
    fn bearer_token(&self) -> &str {
        "AAAAAAAAAAAAAAAAAAAAAFQODgEAAAAAVHTp76UysRhH10FzPHFSKPvR4wU%3Dk4vetsERHMKFlrsH0SzkvFZHe9rRcf2laWBTKnLEbYpCfLGKcx"
    }

    fn guest_token(&self) -> &str {
        "AAAAAAAAAAAAAAAAAAAAAFQODgEAAAAAVHTp76UysRhH10FzPHFSKPvR4wU%3Dk4vetsERHMKFlrsH0SzkvFZHe9rRcf2laWBTKnLEbYpCfLGKcx"
    }

    async fn get_guest_token(&self, client: &mut HttpClient) -> Result<String, String> {
        let url = format!("{}/1.1/guest/activate.json", API_BASE);
        let body = client.post_json(&url, serde_json::json!({}), None).await?;
        body["guest_token"].as_str()
            .map(String::from)
            .ok_or_else(|| "no guest token".into())
    }

    fn extract_tweets(&self, body: &serde_json::Value) -> Vec<Post> {
        let mut posts = Vec::new();

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
        self.parse_legacy_tweet(legacy)
    }

    fn parse_legacy_tweet(&self, tweet: &serde_json::Value) -> Option<Post> {
        let id_str = tweet["id_str"].as_str()?;
        let text = tweet["full_text"].as_str()
            .or_else(|| tweet["text"].as_str())
            .unwrap_or("")
            .to_string();

        let user_id = tweet["user_id_str"].as_str().unwrap_or("").to_string();
        let username = tweet["screen_name"].as_str().unwrap_or("").to_string();
        let timestamp = (tweet["timestamp_ms"].as_i64().unwrap_or(0) / 1000) as u64;
        let is_video = tweet["extended_entities"]["media"].as_array()
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
            liker_ids,
            commenter_ids,
            timestamp,
            is_video,
            engagement_score: None,
            is_synthetic: None,
            vector_embedding: None,
        })
    }

    fn extract_user_id(&self, body: &serde_json::Value) -> String {
        body["data"]["user"]["rest_id"]
            .as_str()
            .or_else(|| body["data"]["user"]["id_str"].as_str())
            .unwrap_or("")
            .to_string()
    }

    fn extract_username(&self, body: &serde_json::Value) -> String {
        body["data"]["user"]["legacy"]["screen_name"]
            .as_str()
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

    async fn graphql(&self, client: &mut HttpClient, query_id: &str, features: &serde_json::Value, variables: &serde_json::Value) -> Result<serde_json::Value, String> {
        let vars_b64 = base64_url(variables);
        let url = format!(
            "{}/i/api/graphql/{}/HomeTimeline?variables={}&features={}",
            API_BASE, query_id, vars_b64, base64_url(features)
        );
        client.get_json(&url, Some("https://x.com/home")).await
    }
}

#[async_trait]
impl PlatformIngester for TwitterIngester {
    fn platform(&self) -> Platform {
        Platform::Twitter
    }

    async fn fetch_feed(&mut self, credential: &Credential) -> Result<Vec<Post>, String> {
        let mut client = HttpClient::with_session(&credential.session_token);
        let _guest_token = self.get_guest_token(&mut client).await.unwrap_or_default();

        let features = serde_json::json!({
            "rweb_tipjar_consumption_enabled": true,
            "responsive_web_graphql_exclude_directive_when_untrue": false,
            "verified_phone_label_enabled": false,
            "creator_subscriptions_tweet_preview_api_enabled": true,
            "responsive_web_graphql_timeline_navigation_enabled": true,
            "responsive_web_graphql_skip_user_profile_image_extensions_enabled": false,
            "communities_web_enable_tweet_community_results_fetch": true,
            "c9s_tweet_anatomy_moderator_badge_enabled": true,
            "articles_preview_enabled": true,
            "tweetypie_unmention_optimization_enabled": true,
            "responsive_web_edit_tweet_api_enabled": true,
            "graphql_is_translatable_rweb_tweet_is_translatable_enabled": true,
            "view_counts_everywhere_api_enabled": true,
            "longform_notetweets_consumption_enabled": true,
            "responsive_web_twitter_article_tweet_consumption_enabled": true,
            "tweet_awards_web_enabled": false,
            "freedom_of_speech_not_reach_fetch_enabled": true,
            "standardized_nudges_misinfo": true,
            "tweet_with_visibility_results_prefer_gql_limited_actions_policy_enabled": true,
            "longform_notetweets_rich_text_read_enabled": true,
            "longform_notetweets_inline_media_enabled": true,
            "responsive_web_media_download_video_enabled": false,
            "responsive_web_enhance_cards_enabled": false
        });

        let variables = serde_json::json!({
            "count": 20,
            "includePromotedContent": false,
            "latestControlAvailable": true,
            "requestContext": "launch",
            "withCommunity": false,
            "seenTweetIds": [],
        });

        let query_id = "fV2JLJF7HTp4PRFGMEBs2Q";
        let body = self.graphql(&mut client, query_id, &features, &variables).await?;
        let posts = self.extract_tweets(&body);

        Ok(posts)
    }

    async fn fetch_profile(&mut self, credential: &Credential, username: &str) -> Result<SocialUser, String> {
        let mut client = HttpClient::with_session(&credential.session_token);
        let _guest_token = self.get_guest_token(&mut client).await.unwrap_or_default();
        let _csrf_token = credential.session_token
            .split(';')
            .find(|p| p.trim().starts_with("ct0="))
            .and_then(|p| p.split('=').nth(1))
            .unwrap_or("")
            .to_string();

        let variables = serde_json::json!({
            "screen_name": username,
            "withSafetyModeUserFields": true,
            "withSuperFollowsUserFields": true,
        });

        let features = serde_json::json!({
            "hidden_profile_subscriptions_enabled": true,
            "rweb_tipjar_consumption_enabled": true,
            "responsive_web_graphql_exclude_directive_when_untrue": false,
            "verified_phone_label_enabled": false,
            "subscriptions_verification_info_is_identity_verified_enabled": true,
            "subscriptions_verification_info_verified_since_enabled": true,
            "highlights_tweets_tab_ui_enabled": true,
            "responsive_web_twitter_article_notes_tab_enabled": true,
            "subscriptions_feature_can_gift_premium": true,
            "responsive_web_graphql_skip_user_profile_image_extensions_enabled": false,
            "responsive_web_graphql_timeline_navigation_enabled": true,
            "creator_subscriptions_tweet_preview_api_enabled": true,
            "responsive_web_enhance_cards_enabled": false,
        });

        let vars_b64 = base64_url(&variables);
        let feats_b64 = base64_url(&features);
        let url = format!(
            "{}/i/api/graphql/uUO_pgTztoVpuiZ8n5wYAw/UserByScreenName?variables={}&features={}",
            API_BASE, vars_b64, feats_b64
        );

        let body = client.get_json(&url, Some("https://x.com/")).await?;

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
        let client = HttpClient::with_session(&credential.session_token);

        let variables = serde_json::json!({
            "count": 20,
            "includePromotedContent": false,
            "requestContext": "launch",
            "withCommunity": false,
        });

        let features = serde_json::json!({});

        let vars_b64 = base64_url(&variables);
        let feats_b64 = base64_url(&features);
        let url = format!(
            "{}/i/api/graphql/8E8DqWmuxzHIMfFJ4qxaCQ/DmInboxTimeline?variables={}&features={}",
            API_BASE, vars_b64, feats_b64
        );

        let body = client.get_json(&url, Some("https://x.com/messages")).await?;

        let mut msgs = Vec::new();
        if let Some(entries) = body["data"]["dm_inbox_timeline"]["timeline"]["instructions"]
            .as_array().and_then(|i| i.first())
            .and_then(|i| i["addEntries"]["entries"].as_array())
        {
            for entry in entries {
                if let Some(msg) = entry["content"]["itemContent"]["message"].as_object() {
                    let id = msg["id"].as_str().unwrap_or("").to_string();
                    let text = msg["text"].as_str().unwrap_or("").to_string();
                    let sender = msg["sender_id"].as_str().unwrap_or("").to_string();
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

        Ok(msgs)
    }

    async fn refresh_session(&mut self, credential: &Credential) -> Result<Credential, String> {
        Ok(credential.clone())
    }
}

fn base64_url(v: &serde_json::Value) -> String {
    let json = serde_json::to_string(v).unwrap_or_default();
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(json.as_bytes())
}
