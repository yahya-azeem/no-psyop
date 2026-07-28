use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandardMessage {
    pub id: String,
    pub platform: String,
    pub conversation_id: String,
    pub sender_id: String,
    pub content: String,
    pub timestamp: u64,
}

pub struct UnifiedInbox {
    messages: Vec<StandardMessage>,
    conversations: HashMap<String, ConversationState>,
}

#[derive(Default)]
struct ConversationState {
    participants: Vec<String>,
    last_message_at: u64,
    unread_count: usize,
}

impl UnifiedInbox {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            conversations: HashMap::new(),
        }
    }

    pub fn ingest(&mut self, msg: StandardMessage) {
        let ts = msg.timestamp;
        let conv_id = msg.conversation_id.clone();

        let conv = self.conversations.entry(conv_id.clone()).or_default();
        if !conv.participants.contains(&msg.sender_id) {
            conv.participants.push(msg.sender_id.clone());
        }
        if ts > conv.last_message_at {
            conv.last_message_at = ts;
        }
        conv.unread_count += 1;

        self.messages.push(msg);
    }

    pub fn get_conversations(&self) -> Vec<ConversationSummary> {
        let mut convs: Vec<ConversationSummary> = self
            .conversations
            .iter()
            .map(|(id, state)| {
                let messages = self.get_messages_for_conversation(id);
                let preview = messages.first().map(|m| m.content.clone()).unwrap_or_default();
                ConversationSummary {
                    id: id.clone(),
                    participants: state.participants.clone(),
                    last_message_at: state.last_message_at,
                    preview,
                    unread: state.unread_count,
                }
            })
            .collect();

        convs.sort_by(|a, b| b.last_message_at.cmp(&a.last_message_at));
        convs
    }

    pub fn get_conversation(&self, conversation_id: &str) -> Vec<&StandardMessage> {
        self.messages
            .iter()
            .filter(|m| m.conversation_id == conversation_id)
            .collect()
    }

    fn get_messages_for_conversation(&self, conversation_id: &str) -> Vec<&StandardMessage> {
        self.get_conversation(conversation_id)
    }

    pub fn mark_read(&mut self, conversation_id: &str) {
        if let Some(conv) = self.conversations.get_mut(conversation_id) {
            conv.unread_count = 0;
        }
    }

    pub fn total_messages(&self) -> usize {
        self.messages.len()
    }

    pub fn total_conversations(&self) -> usize {
        self.conversations.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSummary {
    pub id: String,
    pub participants: Vec<String>,
    pub last_message_at: u64,
    pub preview: String,
    pub unread: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(id: &str, conv: &str, sender: &str, content: &str, ts: u64) -> StandardMessage {
        StandardMessage {
            id: id.into(),
            platform: "Twitter".into(),
            conversation_id: conv.into(),
            sender_id: sender.into(),
            content: content.into(),
            timestamp: ts,
        }
    }

    #[test]
    fn test_ingest_and_retrieve() {
        let mut inbox = UnifiedInbox::new();

        inbox.ingest(make_msg("m1", "conv1", "alice", "hello", 100));
        inbox.ingest(make_msg("m2", "conv1", "bob", "hi back", 200));
        inbox.ingest(make_msg("m3", "conv2", "charlie", "hey", 150));

        assert_eq!(inbox.total_messages(), 3);
        assert_eq!(inbox.total_conversations(), 2);

        let conv_msgs = inbox.get_conversation("conv1");
        assert_eq!(conv_msgs.len(), 2);

        let summaries = inbox.get_conversations();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].id, "conv1");
        assert_eq!(summaries[0].preview, "hello");
    }

    #[test]
    fn test_mark_read() {
        let mut inbox = UnifiedInbox::new();
        inbox.ingest(make_msg("m1", "conv1", "alice", "msg", 100));

        let summaries = inbox.get_conversations();
        assert_eq!(summaries[0].unread, 1);

        inbox.mark_read("conv1");
        let summaries = inbox.get_conversations();
        assert_eq!(summaries[0].unread, 0);
    }

    #[test]
    fn test_ordering() {
        let mut inbox = UnifiedInbox::new();
        inbox.ingest(make_msg("m1", "old", "a", "old", 100));
        inbox.ingest(make_msg("m2", "new", "b", "new", 300));
        inbox.ingest(make_msg("m3", "mid", "c", "mid", 200));

        let convs = inbox.get_conversations();
        assert_eq!(convs[0].id, "new");
        assert_eq!(convs[1].id, "mid");
        assert_eq!(convs[2].id, "old");
    }
}
