use async_trait::async_trait;
use std::collections::HashMap;
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
        let timestamp = (tweet["timestamp_ms"].as_i64()
            .or_else(|| tweet["timestamp_ms"].as_str().and_then(|s| s.parse::<i64>().ok()))
            .unwrap_or(0) / 1000) as u64;
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

    fn ct0(&self, credential: &Credential) -> String {
        credential.session_token
            .split(';')
            .find(|p| p.trim().starts_with("ct0="))
            .and_then(|p| p.split('=').nth(1))
            .unwrap_or("")
            .trim()
            .to_string()
    }

    fn session_gt(&self, credential: &Credential) -> Option<String> {
        credential.session_token
            .split(';')
            .find(|p| p.trim().starts_with("gt="))
            .and_then(|p| p.split('=').nth(1))
            .map(|s| s.trim().to_string())
    }

    async fn guest_token_for(&self, client: &mut HttpClient, credential: &Credential) -> String {
        if let Some(gt) = self.session_gt(credential) {
            return gt;
        }
        self.get_guest_token(client).await.unwrap_or_default()
    }

    fn gql_headers(&self, guest_token: &str, ct0: &str, authenticated: bool) -> Vec<(&'static str, String)> {
        let mut headers = vec![
            ("x-guest-token", guest_token.to_string()),
            ("x-csrf-token", ct0.to_string()),
            ("x-twitter-active-user", "yes".to_string()),
            ("x-twitter-client-language", "en".to_string()),
        ];
        // Authenticated sessions authenticate via cookies (auth_token + ct0);
        // the guest bearer only applies to anonymous requests.
        if !authenticated {
            headers.push(("authorization", format!("Bearer {}", self.bearer_token())));
        }
        headers
    }

    fn is_authenticated(&self, credential: &Credential) -> bool {
        credential.session_token
            .split(';')
            .any(|p| p.trim().starts_with("auth_token="))
    }

    async fn graphql(&self, client: &mut HttpClient, query_id: &str, name: &str, features: &serde_json::Value, variables: &serde_json::Value, guest_token: &str, ct0: &str, authenticated: bool) -> Result<serde_json::Value, String> {
        let url = format!(
            "{}/i/api/graphql/{}/{}",
            GTG_BASE, query_id, name
        );
        let extra = self.gql_headers(guest_token, ct0, authenticated);
        let body = serde_json::json!({
            "variables": variables,
            "queryId": query_id,
            "features": features,
        });
        client.post_json_headers(&url, body, Some("https://x.com/home"), &extra).await
    }
}

#[async_trait]
impl PlatformIngester for TwitterIngester {
    fn platform(&self) -> Platform {
        Platform::Twitter
    }

    async fn fetch_feed(&mut self, credential: &Credential) -> Result<Vec<Post>, String> {
        let mut client = HttpClient::with_session(&credential.session_token);
        let guest_token = self.guest_token_for(&mut client, credential).await;
        let ct0 = self.ct0(credential);

        let features = serde_json::json!({
            "rweb_video_screen_enabled": false,
            "rweb_cashtags_enabled": true,
            "profile_label_improvements_pcf_label_in_post_enabled": true,
            "responsive_web_profile_redirect_enabled": false,
            "rweb_tipjar_consumption_enabled": false,
            "verified_phone_label_enabled": false,
            "creator_subscriptions_tweet_preview_api_enabled": true,
            "responsive_web_graphql_timeline_navigation_enabled": true,
            "responsive_web_graphql_skip_user_profile_image_extensions_enabled": false,
            "premium_content_api_read_enabled": false,
            "communities_web_enable_tweet_community_results_fetch": true,
            "c9s_tweet_anatomy_moderator_badge_enabled": true,
            "responsive_web_grok_analyze_button_fetch_trends_enabled": false,
            "responsive_web_grok_analyze_post_followups_enabled": true,
            "rweb_cashtags_composer_attachment_enabled": true,
            "responsive_web_jetfuel_frame": true,
            "responsive_web_grok_share_attachment_enabled": true,
            "responsive_web_grok_annotations_enabled": true,
            "articles_preview_enabled": true,
            "responsive_web_edit_tweet_api_enabled": true,
            "rweb_conversational_replies_downvote_enabled": false,
            "graphql_is_translatable_rweb_tweet_is_translatable_enabled": true,
            "view_counts_everywhere_api_enabled": true,
            "longform_notetweets_consumption_enabled": true,
            "responsive_web_twitter_article_tweet_consumption_enabled": true,
            "content_disclosure_indicator_enabled": true,
            "content_disclosure_ai_generated_indicator_enabled": true,
            "responsive_web_grok_show_grok_translated_post": true,
            "responsive_web_grok_analysis_button_from_backend": true,
            "post_ctas_fetch_enabled": true,
            "freedom_of_speech_not_reach_fetch_enabled": true,
            "standardized_nudges_misinfo": true,
            "tweet_with_visibility_results_prefer_gql_limited_actions_policy_enabled": true,
            "longform_notetweets_rich_text_read_enabled": true,
            "longform_notetweets_inline_media_enabled": false,
            "responsive_web_grok_image_annotation_enabled": true,
            "responsive_web_grok_imagine_annotation_enabled": true,
            "responsive_web_grok_community_note_auto_translation_is_enabled": true,
            "responsive_web_enhance_cards_enabled": false
        });

        let variables = serde_json::json!({
            "count": 20,
            "includePromotedContent": false,
            "latestControlAvailable": true,
            "requestContext": "launch",
            "seenTweetIds": [],
            "withCommunity": true,
        });

        let query_id = "7zlnp2TxC044W4C1ZUJMHw";
        let body = self.graphql(&mut client, query_id, "HomeTimeline", &features, &variables, &guest_token, &ct0, self.is_authenticated(credential)).await?;
        let posts = self.extract_tweets(&body);

        Ok(posts)
    }

    async fn fetch_profile(&mut self, credential: &Credential, username: &str) -> Result<SocialUser, String> {
        let mut client = HttpClient::with_session(&credential.session_token);
        let guest_token = self.guest_token_for(&mut client, credential).await;
        let ct0 = self.ct0(credential);

        let variables = serde_json::json!({
            "screen_name": username,
            "withSafetyModeUserFields": true,
            "withSuperFollowsUserFields": true,
        });

        let features = serde_json::json!({
            "hidden_profile_subscriptions_enabled": true,
            "profile_label_improvements_pcf_label_in_post_enabled": true,
            "responsive_web_profile_redirect_enabled": false,
            "rweb_tipjar_consumption_enabled": false,
            "verified_phone_label_enabled": false,
            "subscriptions_verification_info_is_identity_verified_enabled": true,
            "subscriptions_verification_info_verified_since_enabled": true,
            "highlights_tweets_tab_ui_enabled": true,
            "responsive_web_twitter_article_notes_tab_enabled": true,
            "subscriptions_feature_can_gift_premium": true,
            "creator_subscriptions_tweet_preview_api_enabled": true,
            "responsive_web_graphql_skip_user_profile_image_extensions_enabled": false,
            "responsive_web_graphql_timeline_navigation_enabled": true,
        });

        let vars_b64 = base64_url(&variables);
        let feats_b64 = base64_url(&features);
        let url = format!(
            "{}/i/api/graphql/IGgvgiOx4QZndDHuD3x9TQ/UserByScreenName?variables={}&features={}",
            GTG_BASE, vars_b64, feats_b64
        );

        let extra = self.gql_headers(&guest_token, &ct0, self.is_authenticated(credential));
        let body = client.get_json_headers(&url, Some("https://x.com/"), &extra).await?;

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
        let mut client = HttpClient::with_session(&credential.session_token);
        let guest_token = self.guest_token_for(&mut client, credential).await;
        let ct0 = self.ct0(credential);

        let variables = serde_json::json!({});
        let features = serde_json::json!({});

        let vars_b64 = base64_url(&variables);
        let feats_b64 = base64_url(&features);
        let url = format!(
            "{}/i/api/graphql/sIC-NZ_cqXLO_WH4jDWFQA/DMPinnedInboxQuery?variables={}&features={}",
            GTG_BASE, vars_b64, feats_b64
        );

        let extra = self.gql_headers(&guest_token, &ct0, self.is_authenticated(credential));
        let body = client.get_json_headers(&url, Some("https://x.com/messages"), &extra).await?;

        Ok(self.extract_messages(&body))
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
    fn test_extract_tweets_home_timeline_ur() {
        let body = serde_json::json!({
            "data": { "home": { "home_timeline_ur": { "instructions": [
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
    fn test_extract_tweets_home_timeline_urt() {
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

    #[test]
    fn test_base64_url() {
        let encoded = base64_url(&serde_json::json!({ "count": 20 }));
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("valid base64");
        assert_eq!(decoded, br#"{"count":20}"#);
    }
}
