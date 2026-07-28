pub mod inbox;

use crate::ingestion::IngestionEngine;
use crate::types::{Credential, Message, Platform};

pub struct UnifiedBridge {
    engine: IngestionEngine,
}

impl UnifiedBridge {
    pub fn new() -> Self {
        Self {
            engine: IngestionEngine::new(),
        }
    }

    pub async fn poll_all_messages(&mut self, creds: &[Credential]) -> Vec<Message> {
        let mut all = Vec::new();

        for ingester in &mut self.engine.ingesters {
            let platform = ingester.platform();
            if let Some(cred) = creds.iter().find(|c| c.platform == platform) {
                match ingester.fetch_messages(cred).await {
                    Ok(msgs) => all.extend(msgs),
                    Err(e) => log::warn!("failed to fetch messages for {:?}: {}", platform, e),
                }
            }
        }

        all.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        all
    }

    pub async fn poll_platform_messages(&mut self, platform: &Platform, cred: &Credential) -> Result<Vec<Message>, String> {
        for ingester in &mut self.engine.ingesters {
            if ingester.platform() == *platform {
                return ingester.fetch_messages(cred).await;
            }
        }
        Err(format!("no ingester for {:?}", platform))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Message, Platform};

    #[test]
    fn test_message_ordering() {
        let mut msgs = vec![
            Message {
                id: "1".into(),
                platform: Platform::Twitter,
                conversation_id: "conv".into(),
                sender_id: "a".into(),
                content: "old".into(),
                timestamp: 100,
            },
            Message {
                id: "2".into(),
                platform: Platform::Twitter,
                conversation_id: "conv".into(),
                sender_id: "b".into(),
                content: "new".into(),
                timestamp: 200,
            },
        ];

        msgs.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        assert_eq!(msgs[0].id, "2");
        assert_eq!(msgs[1].id, "1");
    }

    #[test]
    fn test_standardize_message() {
        let msg = Message {
            id: "m1".into(),
            platform: Platform::Instagram,
            conversation_id: "c1".into(),
            sender_id: "u1".into(),
            content: "hello".into(),
            timestamp: 1000,
        };

        let standard = inbox::StandardMessage {
            id: msg.id.clone(),
            platform: format!("{:?}", msg.platform),
            conversation_id: msg.conversation_id.clone(),
            sender_id: msg.sender_id.clone(),
            content: msg.content.clone(),
            timestamp: msg.timestamp,
        };

        assert_eq!(standard.platform, "Instagram");
        assert_eq!(standard.content, "hello");
    }
}
