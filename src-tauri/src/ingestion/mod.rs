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

    async fn refresh_session(&mut self, credential: &Credential) -> Result<Credential, String>;
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
}
