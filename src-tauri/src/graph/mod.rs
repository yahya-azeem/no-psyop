mod db;

pub use db::SocialGraph;

use crate::types::{Conversation, Message, Platform, Post, SocialUser, UserId};
use std::collections::HashSet;
use std::sync::Mutex;

pub struct GraphEngine {
    db: Mutex<SocialGraph>,
}

impl GraphEngine {
    pub fn new(db_path: &str) -> Result<Self, String> {
        let db = SocialGraph::open(db_path).map_err(|e| e.to_string())?;
        Ok(Self { db: Mutex::new(db) })
    }

    pub fn sync_user(&self, user: SocialUser) -> Result<(), String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        db.upsert_user(&user).map_err(|e| e.to_string())
    }

    pub fn get_mutuals(&self, user_id: &UserId, platform: &Platform) -> Result<Vec<SocialUser>, String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        db.get_mutual_connections(user_id, platform).map_err(|e| e.to_string())
    }

    pub fn is_mutual_engagement(&self, post: &Post) -> Result<bool, String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        let mutuals: HashSet<UserId> = db
            .get_mutual_ids(&post.author_id, &post.platform)
            .map_err(|e| e.to_string())?
            .into_iter()
            .collect();

        let interacting: HashSet<UserId> = post
            .liker_ids
            .iter()
            .chain(post.commenter_ids.iter())
            .cloned()
            .collect();

        Ok(!interacting.is_disjoint(&mutuals))
    }

    pub fn proximity_score(&self, user_id: &UserId, post: &Post) -> Result<f32, String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        db.calculate_proximity(user_id, post).map_err(|e| e.to_string())
    }

    pub fn save_post(&self, post: &Post) -> Result<(), String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        db.save_post(post).map_err(|e| e.to_string())
    }

    pub fn save_message(&self, msg: &Message) -> Result<(), String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        db.save_message(msg).map_err(|e| e.to_string())
    }

    pub fn get_feed(&self, platform: &Platform, limit: usize) -> Result<Vec<Post>, String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        db.get_posts_by_proximity(platform, limit).map_err(|e| e.to_string())
    }

    pub fn get_conversations(&self, platform: &Platform) -> Result<Vec<Conversation>, String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        db.get_conversations(platform).map_err(|e| e.to_string())
    }

    pub fn get_all_conversations(&self) -> Result<Vec<Conversation>, String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        db.get_all_conversations().map_err(|e| e.to_string())
    }

    pub fn get_messages(&self, conversation_id: &str, platform: &Platform) -> Result<Vec<Message>, String> {
        let db = self.db.lock().map_err(|e| e.to_string())?;
        db.get_messages(conversation_id, platform).map_err(|e| e.to_string())
    }
}
