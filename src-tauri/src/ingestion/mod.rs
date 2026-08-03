pub mod instagram;
pub mod twitter;
pub mod linkedin;

use crate::types::{Credential, Platform, Post, SocialUser};
use async_trait::async_trait;

#[async_trait]
pub trait PlatformIngester {
    fn platform(&self) -> Platform;

    async fn fetch_feed(&mut self, credential: &Credential) -> Result<Vec<Post>, String>;

    async fn fetch_profile(&mut self, credential: &Credential, username: &str) -> Result<SocialUser, String>;

    async fn fetch_messages(&mut self, credential: &Credential) -> Result<Vec<crate::types::Message>, String>;

    async fn fetch_inbox(&mut self, credential: &Credential) -> Result<Vec<(crate::types::Conversation, Vec<crate::types::Message>)>, String> {
        let msgs = self.fetch_messages(credential).await?;
        Ok(group_messages_by_conversation(msgs))
    }

    async fn refresh_session(&mut self, credential: &Credential) -> Result<Credential, String>;
}

fn group_messages_by_conversation(msgs: Vec<crate::types::Message>) -> Vec<(crate::types::Conversation, Vec<crate::types::Message>)> {
    use std::collections::BTreeMap;
    let mut by_conv: BTreeMap<String, (crate::types::Platform, Vec<crate::types::Message>, u64)> = BTreeMap::new();
    for m in msgs {
        let entry = by_conv
            .entry(m.conversation_id.clone())
            .or_insert_with(|| (m.platform.clone(), Vec::new(), 0));
        entry.1.push(m.clone());
        entry.2 = entry.2.max(m.timestamp);
    }

    by_conv
        .into_iter()
        .map(|(id, (platform, msgs, last_message_at))| {
            let mut participants: Vec<String> = msgs
                .iter()
                .filter(|m| m.sender_id != "You")
                .map(|m| m.sender_id.clone())
                .collect();
            participants.sort();
            participants.dedup();
            if participants.is_empty() {
                participants.push("Unknown".into());
            }
            let conv = crate::types::Conversation {
                id,
                platform,
                participants,
                last_message_at,
                unread: false,
            };
            (conv, msgs)
        })
        .collect()
}

pub struct IngestionEngine {
    pub ingesters: Vec<Box<dyn PlatformIngester + Send + Sync>>,
}

impl IngestionEngine {
    pub fn new() -> Self {
        Self {
            ingesters: vec![
                Box::new(instagram::InstagramIngester),
                Box::new(twitter::TwitterIngester),
                Box::new(linkedin::LinkedInIngester),
            ],
        }
    }

    pub async fn fetch_all_feeds(&mut self, creds: &[Credential]) -> Vec<(Platform, Result<Vec<Post>, String>)> {
        let mut results = Vec::new();
        for ingester in &mut self.ingesters {
            let platform = ingester.platform();
            if let Some(cred) = creds.iter().find(|c| c.platform == platform) {
                let result = ingester.fetch_feed(cred).await;
                results.push((platform, result));
            }
        }
        results
    }

    pub async fn fetch_all_inboxes(
        &mut self,
        creds: &[Credential],
    ) -> Vec<(Platform, Result<Vec<(crate::types::Conversation, Vec<crate::types::Message>)>, String>)> {
        let mut results = Vec::new();
        for ingester in &mut self.ingesters {
            let platform = ingester.platform();
            if let Some(cred) = creds.iter().find(|c| c.platform == platform) {
                let result = ingester.fetch_inbox(cred).await;
                results.push((platform, result));
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, conv: &str, sender: &str, ts: u64) -> crate::types::Message {
        crate::types::Message {
            id: id.to_string(),
            platform: Platform::LinkedIn,
            conversation_id: conv.to_string(),
            sender_id: sender.to_string(),
            content: "hello".into(),
            timestamp: ts,
        }
    }

    #[test]
    fn test_group_messages_by_conversation_buckets_and_sorts() {
        let msgs = vec![
            msg("e1", "urn:li:thread:a", "100", 3),
            msg("e2", "urn:li:thread:b", "200", 5),
            msg("e3", "urn:li:thread:a", "101", 7),
            msg("e4", "urn:li:thread:a", "100", 9),
        ];

        let threads = group_messages_by_conversation(msgs);
        assert_eq!(threads.len(), 2);

        let a = threads.iter().find(|(c, _)| c.id == "urn:li:thread:a").expect("thread a");
        assert_eq!(a.1.len(), 3);
        assert_eq!(a.1.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), vec!["e1", "e3", "e4"]);
        assert_eq!(a.0.last_message_at, 9);
        assert_eq!(a.0.platform, Platform::LinkedIn);
        let mut parts = a.0.participants.clone();
        parts.sort();
        assert_eq!(parts, vec!["100", "101"]);

        let b = threads.iter().find(|(c, _)| c.id == "urn:li:thread:b").expect("thread b");
        assert_eq!(b.1.len(), 1);
    }

    #[test]
    fn test_group_ignores_you_sender() {
        let msgs = vec![
            msg("e1", "c1", "You", 1),
            msg("e2", "c1", "300", 2),
        ];
        let threads = group_messages_by_conversation(msgs);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].0.participants, vec!["300"]);
    }

    #[test]
    fn test_group_unknown_participant_fallback() {
        let msgs = vec![msg("e1", "c1", "You", 1)];
        let threads = group_messages_by_conversation(msgs);
        assert_eq!(threads.len(), 1);
        assert_eq!(threads[0].0.participants, vec!["Unknown"]);
    }

    #[test]
    fn test_group_empty() {
        assert!(group_messages_by_conversation(Vec::new()).is_empty());
    }
}
