use serde::{Deserialize, Serialize};

pub type UserId = String;
pub type PostId = String;
pub type SessionId = String;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Platform {
    Instagram,
    Twitter,
    LinkedIn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Credential {
    pub platform: Platform,
    pub session_token: String,
    pub user_id: UserId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialUser {
    pub id: UserId,
    pub platform: Platform,
    pub username: String,
    pub follows: Vec<UserId>,
    pub followers: Vec<UserId>,
    pub last_sync: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryItem {
    pub id: String,
    pub media_type: u8,
    pub media_url: String,
    pub poster_url: Option<String>,
    pub is_video: bool,
    pub timestamp: u64,
    pub expiring_at: u64,
    pub caption: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoryUser {
    pub id: UserId,
    pub username: String,
    pub profile_pic_url: String,
    pub is_mutual: bool,
    pub is_close_friend: bool,
    pub items: Vec<StoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Post {
    pub id: PostId,
    pub platform: Platform,
    pub author_id: UserId,
    pub author_username: String,
    pub content: String,
    pub media_urls: Vec<String>,
    pub poster_url: Option<String>,
    pub liker_ids: Vec<UserId>,
    pub commenter_ids: Vec<UserId>,
    pub timestamp: u64,
    pub is_video: bool,
    pub author_is_mutual: Option<bool>,
    pub author_is_close_friend: Option<bool>,
    pub engagement_score: Option<f32>,
    pub is_synthetic: Option<bool>,
    pub vector_embedding: Option<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub platform: Platform,
    pub conversation_id: String,
    pub sender_id: UserId,
    pub content: String,
    pub timestamp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub id: String,
    pub post_id: PostId,
    pub platform: Platform,
    pub author_id: UserId,
    pub author_username: String,
    pub content: String,
    pub timestamp: u64,
    pub likes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedItem {
    pub post: Post,
    pub proximity_score: f32,
    pub relevance_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Digest {
    pub clusters: Vec<ContentCluster>,
    pub generated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentCluster {
    pub topic: String,
    pub items: Vec<Post>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: String,
    pub platform: Platform,
    pub participants: Vec<UserId>,
    pub last_message_at: u64,
    pub unread: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub posts_added: usize,
    pub messages_added: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserIntent {
    pub query: String,
    pub vector: Option<Vec<f32>>,
    pub platforms: Vec<Platform>,
    pub max_results: usize,
}
