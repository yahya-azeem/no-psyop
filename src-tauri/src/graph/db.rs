use rusqlite::{params, Connection, Result};

use crate::types::{Platform, Post, SocialUser, UserId};
use proximity::{ProximityConfig, ProximityWeights};

pub struct SocialGraph {
    conn: Connection,
    weights: ProximityWeights,
}

impl SocialGraph {
    pub fn open(path: &str) -> Result<Self> {
        let conn = if path.is_empty() {
            Connection::open_in_memory()?
        } else {
            Connection::open(path)?
        };
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        if !path.is_empty() {
            let _ = conn.pragma_update(None, "journal_mode", "WAL");
            let _ = conn.pragma_update(None, "synchronous", "NORMAL");
        }
        let db = Self {
            conn,
            weights: ProximityWeights::default(),
        };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT NOT NULL,
                platform INTEGER NOT NULL,
                username TEXT NOT NULL,
                follows TEXT NOT NULL DEFAULT '[]',
                followers TEXT NOT NULL DEFAULT '[]',
                last_sync INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (id, platform)
            );

            CREATE TABLE IF NOT EXISTS interactions (
                user_id TEXT NOT NULL,
                platform INTEGER NOT NULL,
                post_id TEXT NOT NULL,
                interaction_type INTEGER NOT NULL,
                timestamp INTEGER NOT NULL,
                PRIMARY KEY (user_id, platform, post_id, interaction_type)
            );

            CREATE TABLE IF NOT EXISTS posts (
                id TEXT NOT NULL,
                platform INTEGER NOT NULL,
                author_id TEXT NOT NULL,
                author_username TEXT NOT NULL,
                content TEXT NOT NULL,
                media_urls TEXT NOT NULL DEFAULT '[]',
                poster_url TEXT,
                liker_ids TEXT NOT NULL DEFAULT '[]',
                commenter_ids TEXT NOT NULL DEFAULT '[]',
                timestamp INTEGER NOT NULL,
                is_video INTEGER NOT NULL DEFAULT 0,
                author_is_mutual INTEGER,
                author_is_close_friend INTEGER,
                is_synthetic INTEGER,
                engagement_score REAL,
                seen INTEGER NOT NULL DEFAULT 0,
                proximity_score REAL DEFAULT 0.0,
                PRIMARY KEY (id, platform)
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT NOT NULL,
                platform INTEGER NOT NULL,
                conversation_id TEXT NOT NULL,
                sender_id TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                PRIMARY KEY (id, platform)
            );

            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT NOT NULL,
                platform INTEGER NOT NULL,
                participants TEXT NOT NULL DEFAULT '[]',
                last_message_at INTEGER NOT NULL DEFAULT 0,
                unread INTEGER NOT NULL DEFAULT 0,
                PRIMARY KEY (id, platform)
            );

            CREATE INDEX IF NOT EXISTS idx_interactions_post ON interactions(post_id);
            CREATE INDEX IF NOT EXISTS idx_interactions_user ON interactions(user_id);
            CREATE INDEX IF NOT EXISTS idx_posts_platform ON posts(platform);
            CREATE INDEX IF NOT EXISTS idx_posts_timestamp ON posts(timestamp);
            CREATE INDEX IF NOT EXISTS idx_messages_conv ON messages(conversation_id);
            CREATE INDEX IF NOT EXISTS idx_messages_platform ON messages(platform);"
        )?;
        self.migrate_columns()
    }

    fn migrate_columns(&self) -> Result<()> {
        let existing: Vec<String> = {
            let mut stmt = self.conn.prepare("PRAGMA table_info(posts)")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
            rows.collect::<Result<Vec<_>>>()?
        };
        for col in ["author_is_mutual", "author_is_close_friend", "seen", "poster_url", "engagement_score"] {
            if !existing.iter().any(|c| c == col) {
                let ty = match col {
                    "poster_url" => "TEXT",
                    "engagement_score" => "REAL",
                    _ => "INTEGER",
                };
                self.conn.execute_batch(&format!(
                    "ALTER TABLE posts ADD COLUMN {} {};",
                    col, ty
                ))?;
            }
        }
        Ok(())
    }

    pub fn upsert_user(&self, user: &SocialUser) -> Result<()> {
        let follows_json = serde_json::to_string(&user.follows).unwrap_or_default();
        let followers_json = serde_json::to_string(&user.followers).unwrap_or_default();
        let platform_int = platform_to_int(&user.platform);

        self.conn.execute(
            "INSERT INTO users (id, platform, username, follows, followers, last_sync)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id, platform) DO UPDATE SET
                username = excluded.username,
                follows = excluded.follows,
                followers = excluded.followers,
                last_sync = excluded.last_sync",
            params![user.id, platform_int, user.username, follows_json, followers_json, user.last_sync],
        )?;
        Ok(())
    }

    #[allow(dead_code)] // graph analytics API reserved for future ranking features
    pub fn get_mutual_connections(&self, user_id: &UserId, platform: &Platform) -> Result<Vec<SocialUser>> {
        let platform_int = platform_to_int(platform);
        let mut stmt = self.conn.prepare(
            "SELECT u.id, u.platform, u.username, u.follows, u.followers, u.last_sync
             FROM users u
             WHERE u.platform = ?1
               AND u.id != ?2
               AND u.id IN (
                   SELECT value FROM json_each(
                       (SELECT follows FROM users WHERE id = ?2 AND platform = ?1)
                   )
               )
               AND ?2 IN (
                   SELECT value FROM json_each(u.followers)
               )"
        )?;

        let users = stmt.query_map(params![platform_int, user_id], |row| {
            let id: String = row.get(0)?;
            let plat_int: i32 = row.get(1)?;
            let username: String = row.get(2)?;
            let follows_str: String = row.get(3)?;
            let followers_str: String = row.get(4)?;
            let last_sync: u64 = row.get(5)?;

            Ok(SocialUser {
                id,
                platform: int_to_platform(plat_int),
                username,
                follows: serde_json::from_str(&follows_str).unwrap_or_default(),
                followers: serde_json::from_str(&followers_str).unwrap_or_default(),
                last_sync,
            })
        })?;

        users.collect()
    }

    pub fn get_mutual_ids(&self, user_id: &UserId, platform: &Platform) -> Result<Vec<UserId>> {
        let platform_int = platform_to_int(platform);
        let mut stmt = self.conn.prepare(
            "SELECT u.id FROM users u
             WHERE u.platform = ?1
               AND u.id != ?2
               AND u.id IN (
                   SELECT value FROM json_each(
                       (SELECT follows FROM users WHERE id = ?2 AND platform = ?1)
                   )
               )
               AND ?2 IN (
                   SELECT value FROM json_each(u.followers)
               )"
        )?;

        let ids: Vec<UserId> = stmt
            .query_map(params![platform_int, user_id], |row| row.get::<_, String>(0))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(ids)
    }

    #[allow(dead_code)] // graph analytics API reserved for future ranking features
    pub fn record_interaction(&self, user_id: &UserId, platform: &Platform, post_id: &str, interaction_type: u8, timestamp: u64) -> Result<()> {
        let platform_int = platform_to_int(platform);
        self.conn.execute(
            "INSERT OR IGNORE INTO interactions (user_id, platform, post_id, interaction_type, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![user_id, platform_int, post_id, interaction_type, timestamp],
        )?;
        Ok(())
    }

    pub fn save_post(&self, post: &Post) -> Result<()> {
        let platform_int = platform_to_int(&post.platform);
        let media_json = serde_json::to_string(&post.media_urls).unwrap_or_default();
        let liker_json = serde_json::to_string(&post.liker_ids).unwrap_or_default();
        let commenter_json = serde_json::to_string(&post.commenter_ids).unwrap_or_default();

        let mutual_ids = self.get_mutual_ids(&post.author_id, &post.platform).unwrap_or_default();
        let proximity = self.calculate_proximity_raw(&post, &mutual_ids);

        self.conn.execute(
            "INSERT OR REPLACE INTO posts (id, platform, author_id, author_username, content,
             media_urls, poster_url, liker_ids, commenter_ids, timestamp, is_video,
             author_is_mutual, author_is_close_friend, is_synthetic, engagement_score, proximity_score)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                post.id, platform_int, post.author_id, post.author_username, post.content,
                media_json, post.poster_url, liker_json, commenter_json, post.timestamp,
                post.is_video as i32,
                post.author_is_mutual.map(|v| v as i32),
                post.author_is_close_friend.map(|v| v as i32),
                post.is_synthetic,
                post.engagement_score,
                proximity,
            ],
        )?;
        Ok(())
    }

    pub fn calculate_proximity(&self, user_id: &UserId, post: &Post) -> Result<f32> {
        let mutual_ids = self.get_mutual_ids(user_id, &post.platform)?;
        Ok(self.calculate_proximity_raw(post, &mutual_ids))
    }

    fn calculate_proximity_raw(&self, post: &Post, mutual_ids: &[UserId]) -> f32 {
        let mut scalars = ProximityScalars::default();
        let mutual_set: std::collections::HashSet<&UserId> = mutual_ids.iter().collect();

        for liker in &post.liker_ids {
            if mutual_set.contains(liker) {
                scalars.mutual_likes += 1;
            } else {
                scalars.non_mutual_likes += 1;
            }
        }

        for commenter in &post.commenter_ids {
            if mutual_set.contains(commenter) {
                scalars.mutual_comments += 1;
            } else {
                scalars.non_mutual_comments += 1;
            }
        }

        scalars.mutual_like_count = scalars.mutual_likes as usize;
        scalars.mutual_comment_count = scalars.mutual_comments as usize;
        scalars.total_likes = (scalars.mutual_likes + scalars.non_mutual_likes) as usize;
        scalars.total_comments = (scalars.mutual_comments + scalars.non_mutual_comments) as usize;

        let now = chrono::Utc::now().timestamp() as u64;
        let age_hours = if post.timestamp > 0 {
            (now - post.timestamp) as f32 / 3600.0
        } else {
            0.0
        };
        scalars.age_hours = age_hours;

        ProximityConfig::score(&self.weights, &scalars)
    }

    pub fn get_posts_by_proximity(&self, platform: &Platform, limit: usize) -> Result<Vec<Post>> {
        let platform_int = platform_to_int(platform);
        let mut stmt = self.conn.prepare(
            "SELECT id, platform, author_id, author_username, content,
                    media_urls, poster_url, liker_ids, commenter_ids, timestamp, is_video,
                    author_is_mutual, author_is_close_friend, is_synthetic, engagement_score
             FROM posts
             WHERE platform = ?1 AND is_synthetic IS NOT 1 AND seen IS NOT 1
             ORDER BY proximity_score DESC, timestamp DESC
             LIMIT ?2"
        )?;

        let posts = stmt.query_map(params![platform_int, limit as i64], |row| {
            let id: String = row.get(0)?;
            let plat_int: i32 = row.get(1)?;
            let author_id: String = row.get(2)?;
            let author_username: String = row.get(3)?;
            let content: String = row.get(4)?;
            let media_str: String = row.get(5)?;
            let poster_url: Option<String> = row.get(6)?;
            let liker_str: String = row.get(7)?;
            let commenter_str: String = row.get(8)?;
            let timestamp: u64 = row.get(9)?;
            let is_video: i32 = row.get(10)?;
            let author_is_mutual: Option<i32> = row.get(11)?;
            let author_is_close_friend: Option<i32> = row.get(12)?;
            let is_synthetic: Option<i32> = row.get(13)?;
            let engagement_score: Option<f32> = row.get(14)?;

            Ok(Post {
                id,
                platform: int_to_platform(plat_int),
                author_id,
                author_username,
                content,
                media_urls: serde_json::from_str(&media_str).unwrap_or_default(),
                poster_url,
                liker_ids: serde_json::from_str(&liker_str).unwrap_or_default(),
                commenter_ids: serde_json::from_str(&commenter_str).unwrap_or_default(),
                timestamp,
                is_video: is_video != 0,
                author_is_mutual: author_is_mutual.map(|v| v != 0),
                author_is_close_friend: author_is_close_friend.map(|v| v != 0),
                engagement_score,
                is_synthetic: is_synthetic.map(|v| v != 0),
                vector_embedding: None,
            })
        })?;

        posts.collect()
    }

    /// Full-text search over a post's content and author, restricted to an
    /// optional platform. Returns newest-first matches (a lightweight FTS
    /// substitute; see the `posts` table schema and `search_posts_text`).
    pub fn search_posts_text(&self, query: &str, platform: Option<&Platform>, limit: usize) -> Result<Vec<Post>> {
        let like = format!("%{}%", query.trim());
        let mut sql = String::from(
            "SELECT id, platform, author_id, author_username, content,
                    media_urls, poster_url, liker_ids, commenter_ids, timestamp, is_video,
                    author_is_mutual, author_is_close_friend, is_synthetic, engagement_score
             FROM posts
             WHERE content LIKE ?1 OR author_username LIKE ?1
             ORDER BY timestamp DESC
             LIMIT ?2",
        );

        let mut stmt = match platform {
            Some(p) => {
                sql = sql.replace("WHERE", &format!("WHERE platform = {} AND", platform_to_int(p)));
                self.conn.prepare(&sql)?
            }
            None => self.conn.prepare(&sql)?,
        };

        let posts = stmt.query_map(params![like, limit as i64], move |row| parse_post_row(row))?;
        posts.collect()
    }

    pub fn save_message(&self, msg: &crate::types::Message) -> Result<()> {
        let platform_int = platform_to_int(&msg.platform);
        self.conn.execute(
            "INSERT OR IGNORE INTO messages (id, platform, conversation_id, sender_id, content, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![msg.id, platform_int, msg.conversation_id, msg.sender_id, msg.content, msg.timestamp],
        )?;
        Ok(())
    }

    pub fn save_conversation(&self, conv: &crate::types::Conversation) -> Result<()> {
        let platform_int = platform_to_int(&conv.platform);
        let participants_json = serde_json::to_string(&conv.participants).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO conversations (id, platform, participants, last_message_at, unread)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![conv.id, platform_int, participants_json, conv.last_message_at, conv.unread as i32],
        )?;
        Ok(())
    }

    pub fn get_conversations(&self, platform: &Platform) -> Result<Vec<crate::types::Conversation>> {
        let platform_int = platform_to_int(platform);
        let mut stmt = self.conn.prepare(
            "SELECT id, platform, participants, last_message_at, unread
             FROM conversations WHERE platform = ?1
             ORDER BY last_message_at DESC"
        )?;

        let convs = stmt.query_map(params![platform_int], |row| {
            let id: String = row.get(0)?;
            let plat_int: i32 = row.get(1)?;
            let participants_str: String = row.get(2)?;
            let last_message_at: u64 = row.get(3)?;
            let unread: i32 = row.get(4)?;

            Ok(crate::types::Conversation {
                id,
                platform: int_to_platform(plat_int),
                participants: serde_json::from_str(&participants_str).unwrap_or_default(),
                last_message_at,
                unread: unread != 0,
            })
        })?;

        convs.collect()
    }

    pub fn get_messages(&self, conversation_id: &str, platform: &Platform) -> Result<Vec<crate::types::Message>> {
        let platform_int = platform_to_int(platform);
        let mut stmt = self.conn.prepare(
            "SELECT id, platform, conversation_id, sender_id, content, timestamp
             FROM messages
             WHERE conversation_id = ?1 AND platform = ?2
             ORDER BY timestamp ASC"
        )?;

        let msgs = stmt.query_map(params![conversation_id, platform_int], |row| {
            let id: String = row.get(0)?;
            let plat_int: i32 = row.get(1)?;
            let conv_id: String = row.get(2)?;
            let sender_id: String = row.get(3)?;
            let content: String = row.get(4)?;
            let timestamp: u64 = row.get(5)?;

            Ok(crate::types::Message {
                id,
                platform: int_to_platform(plat_int),
                conversation_id: conv_id,
                sender_id,
                content,
                timestamp,
            })
        })?;

        msgs.collect()
    }

    pub fn get_all_conversations(&self) -> Result<Vec<crate::types::Conversation>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, platform, participants, last_message_at, unread
             FROM conversations
             ORDER BY last_message_at DESC"
        )?;

        let convs = stmt.query_map([], |row| {
            let id: String = row.get(0)?;
            let plat_int: i32 = row.get(1)?;
            let participants_str: String = row.get(2)?;
            let last_message_at: u64 = row.get(3)?;
            let unread: i32 = row.get(4)?;

            Ok(crate::types::Conversation {
                id,
                platform: int_to_platform(plat_int),
                participants: serde_json::from_str(&participants_str).unwrap_or_default(),
                last_message_at,
                unread: unread != 0,
            })
        })?;

        convs.collect()
    }

    pub fn mark_post_seen(&self, platform: &Platform, post_id: &str) -> Result<()> {
        let platform_int = platform_to_int(platform);
        self.conn.execute(
            "UPDATE posts SET seen = 1 WHERE id = ?1 AND platform = ?2",
            params![post_id, platform_int],
        )?;
        Ok(())
    }
}

fn platform_to_int(p: &Platform) -> i32 {
    match p {
        Platform::Instagram => 0,
        Platform::Twitter => 1,
        Platform::LinkedIn => 2,
    }
}

fn int_to_platform(i: i32) -> Platform {
    match i {
        0 => Platform::Instagram,
        1 => Platform::Twitter,
        _ => Platform::LinkedIn,
    }
}

/// Map a `posts` row (the shared column projection) into a [`Post`]. Kept as a
/// free function so every query over the posts table reuses the same parsing.
fn parse_post_row(row: &rusqlite::Row) -> rusqlite::Result<Post> {
    let id: String = row.get(0)?;
    let plat_int: i32 = row.get(1)?;
    let author_id: String = row.get(2)?;
    let author_username: String = row.get(3)?;
    let content: String = row.get(4)?;
    let media_str: String = row.get(5)?;
    let poster_url: Option<String> = row.get(6)?;
    let liker_str: String = row.get(7)?;
    let commenter_str: String = row.get(8)?;
    let timestamp: u64 = row.get(9)?;
    let is_video: i32 = row.get(10)?;
    let author_is_mutual: Option<i32> = row.get(11)?;
    let author_is_close_friend: Option<i32> = row.get(12)?;
    let is_synthetic: Option<i32> = row.get(13)?;
    let engagement_score: Option<f32> = row.get(14)?;

    Ok(Post {
        id,
        platform: int_to_platform(plat_int),
        author_id,
        author_username,
        content,
        media_urls: serde_json::from_str(&media_str).unwrap_or_default(),
        poster_url,
        liker_ids: serde_json::from_str(&liker_str).unwrap_or_default(),
        commenter_ids: serde_json::from_str(&commenter_str).unwrap_or_default(),
        timestamp,
        is_video: is_video != 0,
        author_is_mutual: author_is_mutual.map(|v| v != 0),
        author_is_close_friend: author_is_close_friend.map(|v| v != 0),
        engagement_score,
        is_synthetic: is_synthetic.map(|v| v != 0),
        vector_embedding: None,
    })
}

#[derive(Default)]
struct ProximityScalars {
    mutual_likes: u32,
    non_mutual_likes: u32,
    mutual_comments: u32,
    non_mutual_comments: u32,
    mutual_like_count: usize,
    mutual_comment_count: usize,
    total_likes: usize,
    total_comments: usize,
    age_hours: f32,
}

mod proximity {
    pub struct ProximityWeights {
        pub mutual_like_weight: f32,
        pub mutual_comment_weight: f32,
        pub non_mutual_penalty: f32,
        pub recency_weight: f32,
        #[allow(dead_code)] // reserved; proximity weighting is partially tuned
        pub engagement_ratio_weight: f32,
    }

    impl Default for ProximityWeights {
        fn default() -> Self {
            Self {
                mutual_like_weight: 0.35,
                mutual_comment_weight: 0.40,
                non_mutual_penalty: 0.15,
                recency_weight: 0.10,
                engagement_ratio_weight: 0.0,
            }
        }
    }

    pub struct ProximityConfig;

    impl ProximityConfig {
        pub fn score(weights: &ProximityWeights, s: &super::ProximityScalars) -> f32 {
            let mutual_like_score = if s.mutual_likes > 0 {
                (s.mutual_likes as f32).min(5.0) / 5.0
            } else {
                0.0
            };

            let mutual_comment_score = if s.mutual_comments > 0 {
                (s.mutual_comments as f32).min(3.0) / 3.0
            } else {
                0.0
            };

            let mut non_mutual_penalty = 0.0f32;
            if s.total_likes + s.total_comments > 0 {
                let non_mutual_ratio = (s.non_mutual_likes + s.non_mutual_comments) as f32
                    / (s.total_likes + s.total_comments) as f32;
                if non_mutual_ratio > 0.7 {
                    non_mutual_penalty = non_mutual_ratio * 0.5;
                }
            }

            let recency_score = if s.age_hours < 1.0 {
                1.0
            } else if s.age_hours < 24.0 {
                1.0 - (s.age_hours / 24.0) * 0.5
            } else if s.age_hours < 168.0 {
                0.5 - ((s.age_hours - 24.0) / 144.0) * 0.4
            } else {
                0.1
            };

            let score = mutual_like_score * weights.mutual_like_weight
                + mutual_comment_score * weights.mutual_comment_weight
                + (1.0 - non_mutual_penalty) * weights.non_mutual_penalty
                + recency_score * weights.recency_weight;

            score.clamp(0.0, 1.0)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_proximity_score_mutual_likes_only() {
            let s = super::super::ProximityScalars {
                mutual_likes: 3,
                mutual_comment_count: 0,
                total_likes: 5,
                age_hours: 0.5,
                ..Default::default()
            };
            let weights = ProximityWeights::default();
            let score = ProximityConfig::score(&weights, &s);
            assert!(score > 0.0);
            assert!(score <= 1.0);
        }

        #[test]
        fn test_proximity_score_no_mutuals() {
            let s = super::super::ProximityScalars {
                mutual_likes: 0,
                mutual_comments: 0,
                non_mutual_likes: 10,
                non_mutual_comments: 5,
                total_likes: 10,
                total_comments: 5,
                age_hours: 72.0,
                ..Default::default()
            };
            let weights = ProximityWeights::default();
            let score = ProximityConfig::score(&weights, &s);
            assert!(score < 0.5);
        }

        #[test]
        fn test_proximity_score_old_post() {
            let s = super::super::ProximityScalars {
                mutual_likes: 1,
                total_likes: 2,
                age_hours: 200.0,
                ..Default::default()
            };
            let weights = ProximityWeights::default();
            let score = ProximityConfig::score(&weights, &s);
            assert!(score < 0.5);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Platform;

    #[test]
    fn test_open_in_memory() {
        let graph = SocialGraph::open("").unwrap();
        assert!(graph.conn.is_autocommit());
    }

    #[test]
    fn test_upsert_and_get_mutuals() {
        let graph = SocialGraph::open("").unwrap();

        let user = SocialUser {
            id: "user1".into(),
            platform: Platform::Instagram,
            username: "testuser".into(),
            follows: vec!["user2".into(), "user3".into()],
            followers: vec!["user2".into()],
            last_sync: 1000,
        };

        graph.upsert_user(&user).unwrap();

        let mutual = SocialUser {
            id: "user2".into(),
            platform: Platform::Instagram,
            username: "mutual".into(),
            follows: vec!["user1".into()],
            followers: vec!["user1".into()],
            last_sync: 1000,
        };
        graph.upsert_user(&mutual).unwrap();

        let non_mutual = SocialUser {
            id: "user3".into(),
            platform: Platform::Instagram,
            username: "nonmutual".into(),
            follows: vec![],
            followers: vec![],
            last_sync: 1000,
        };
        graph.upsert_user(&non_mutual).unwrap();

        let mutuals = graph.get_mutual_ids(&"user1".into(), &Platform::Instagram).unwrap();
        assert_eq!(mutuals, vec!["user2"]);
    }

    #[test]
    fn test_calculate_proximity() {
        let graph = SocialGraph::open("").unwrap();

        let user1 = SocialUser {
            id: "user1".into(),
            platform: Platform::Twitter,
            username: "u1".into(),
            follows: vec!["author1".into(), "mutual1".into()],
            followers: vec!["author1".into(), "mutual1".into()],
            last_sync: 1000,
        };
        graph.upsert_user(&user1).unwrap();

        let author = SocialUser {
            id: "author1".into(),
            platform: Platform::Twitter,
            username: "author".into(),
            follows: vec!["user1".into()],
            followers: vec!["user1".into(), "user2".into()],
            last_sync: 1000,
        };
        graph.upsert_user(&author).unwrap();

        // mutual1: user1 follows mutual1 and mutual1 follows user1 back
        let mutual = SocialUser {
            id: "mutual1".into(),
            platform: Platform::Twitter,
            username: "mutual".into(),
            follows: vec!["user1".into()],
            followers: vec!["user1".into()],
            last_sync: 1000,
        };
        graph.upsert_user(&mutual).unwrap();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();

        let post_with_mutual = Post {
            id: "post1".into(),
            platform: Platform::Twitter,
            author_id: "author1".into(),
            author_username: "author".into(),
            content: "hello".into(),
            media_urls: vec![],
            poster_url: None,
            liker_ids: vec!["mutual1".into(), "author1".into(), "user2".into()],
            commenter_ids: vec!["mutual1".into()],
            timestamp: now - 60,
            is_video: false,
            author_is_mutual: Some(true),
            author_is_close_friend: Some(false),
            engagement_score: None,
            is_synthetic: None,
            vector_embedding: None,
        };

        let score = graph.calculate_proximity(&"user1".into(), &post_with_mutual).unwrap();
        assert!(score > 0.5, "mutual engagement should score high");
    }

    #[test]
    fn test_save_and_retrieve_post() {
        let graph = SocialGraph::open("").unwrap();
        let post = Post {
            id: "p1".into(),
            platform: Platform::Twitter,
            author_id: "a1".into(),
            author_username: "user1".into(),
            content: "test content".into(),
            media_urls: vec![],
            poster_url: None,
            liker_ids: vec!["u1".into(), "u2".into()],
            commenter_ids: vec!["u1".into()],
            timestamp: 1000,
            is_video: false,
            author_is_mutual: Some(true),
            author_is_close_friend: Some(true),
            engagement_score: None,
            is_synthetic: Some(false),
            vector_embedding: None,
        };
        graph.save_post(&post).unwrap();
        let posts = graph.get_posts_by_proximity(&Platform::Twitter, 10).unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].id, "p1");
        assert_eq!(posts[0].liker_ids, vec!["u1".to_string(), "u2".to_string()]);
    }

    #[test]
    fn test_search_posts_text_cross_platform() {
        let graph = SocialGraph::open("").unwrap();

        let mk = |id: &str, platform: Platform, author: &str, content: &str, ts: u64| Post {
            id: id.into(),
            platform,
            author_id: format!("a{}", id),
            author_username: author.into(),
            content: content.into(),
            media_urls: vec![],
            poster_url: None,
            liker_ids: vec![],
            commenter_ids: vec![],
            timestamp: ts,
            is_video: false,
            author_is_mutual: None,
            author_is_close_friend: None,
            engagement_score: None,
            is_synthetic: Some(false),
            vector_embedding: None,
        };

        graph.save_post(&mk("ig1", Platform::Instagram, "alice", "dallas food trucks", 3000)).unwrap();
        graph.save_post(&mk("tw1", Platform::Twitter, "bob", "great tacos in dallas", 2000)).unwrap();
        graph.save_post(&mk("li1", Platform::LinkedIn, "cara", "productivity notes", 1000)).unwrap();

        let all = graph.search_posts_text("dallas", None, 10).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "ig1"); // newest first
        assert_eq!(all[1].id, "tw1");

        let ig = graph.search_posts_text("dallas", Some(&Platform::Instagram), 10).unwrap();
        assert_eq!(ig.len(), 1);
        assert_eq!(ig[0].id, "ig1");

        let by_author = graph.search_posts_text("bob", None, 10).unwrap();
        assert_eq!(by_author.len(), 1);
        assert_eq!(by_author[0].id, "tw1");

        assert!(graph.search_posts_text("nope", None, 10).unwrap().is_empty());
    }

    #[test]
    fn test_save_post_filters_synthetic() {
        let graph = SocialGraph::open("").unwrap();
        let real = Post {
            id: "p1".into(), platform: Platform::Instagram, author_id: "a1".into(),
            author_username: "u1".into(), content: "real".into(), media_urls: vec![], poster_url: None,
            liker_ids: vec![], commenter_ids: vec![], timestamp: 1000,
            is_video: false, author_is_mutual: Some(false), author_is_close_friend: Some(false),
            engagement_score: None, is_synthetic: Some(false), vector_embedding: None,
        };
        let synth = Post {
            id: "p2".into(), platform: Platform::Instagram, author_id: "a2".into(),
            author_username: "u2".into(), content: "fake".into(), media_urls: vec![], poster_url: None,
            liker_ids: vec![], commenter_ids: vec![], timestamp: 1001,
            is_video: false, author_is_mutual: Some(false), author_is_close_friend: Some(false),
            engagement_score: None, is_synthetic: Some(true), vector_embedding: None,
        };
        graph.save_post(&real).unwrap();
        graph.save_post(&synth).unwrap();
        let posts = graph.get_posts_by_proximity(&Platform::Instagram, 10).unwrap();
        assert_eq!(posts.len(), 1, "synthetic posts should be excluded from feed");
        assert_eq!(posts[0].id, "p1");
    }

    #[test]
    fn test_save_and_get_messages() {
        let graph = SocialGraph::open("").unwrap();
        let msg = crate::types::Message {
            id: "m1".into(), platform: Platform::Twitter,
            conversation_id: "conv1".into(), sender_id: "user1".into(),
            content: "hello".into(), timestamp: 1000,
        };
        graph.save_message(&msg).unwrap();
        let msgs = graph.get_messages("conv1", &Platform::Twitter).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "hello");
    }

    #[test]
    fn test_save_conversation_roundtrip() {
        let graph = SocialGraph::open("").unwrap();
        let conv = crate::types::Conversation {
            id: "conv1".into(),
            platform: Platform::Instagram,
            participants: vec!["alice".into(), "bob".into()],
            last_message_at: 1700000000,
            unread: true,
        };
        graph.save_conversation(&conv).unwrap();
        let convs = graph.get_conversations(&Platform::Instagram).unwrap();
        assert_eq!(convs.len(), 1);
        assert_eq!(convs[0].participants, vec!["alice".to_string(), "bob".to_string()]);
        assert!(convs[0].unread);
    }

    #[test]
    fn test_mutual_and_close_friend_roundtrip() {
        let graph = SocialGraph::open("").unwrap();
        let post = Post {
            id: "p2".into(), platform: Platform::Instagram, author_id: "a2".into(),
            author_username: "u2".into(), content: "post".into(), media_urls: vec![], poster_url: None,
            liker_ids: vec![], commenter_ids: vec![], timestamp: 1001,
            is_video: true, author_is_mutual: Some(true), author_is_close_friend: Some(true),
            engagement_score: None, is_synthetic: Some(false), vector_embedding: None,
        };
        graph.save_post(&post).unwrap();
        let posts = graph.get_posts_by_proximity(&Platform::Instagram, 10).unwrap();
        assert_eq!(posts.len(), 1);
        assert_eq!(posts[0].author_is_mutual, Some(true));
        assert_eq!(posts[0].author_is_close_friend, Some(true));
        assert!(posts[0].is_video);
    }

    #[test]
    fn test_get_empty_conversation() {
        let graph = SocialGraph::open("").unwrap();
        let msgs = graph.get_messages("nonexistent", &Platform::LinkedIn).unwrap();
        assert!(msgs.is_empty());
    }
}
