use crate::types::{Credential, Platform};
use std::path::PathBuf;

const SERVICE_NAME: &str = "no_pysop";

pub struct SecureStore {
    dir: PathBuf,
}

impl SecureStore {
    pub fn new() -> Self {
        let dir = dirs_next::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(SERVICE_NAME);
        std::fs::create_dir_all(&dir).ok();
        Self { dir }
    }

    fn file_path(&self, platform: &Platform) -> PathBuf {
        self.dir.join(format!("cred_{:?}.json", platform))
    }

    pub fn store_credential(&self, cred: &Credential) -> Result<(), String> {
        let json = serde_json::to_string(cred).map_err(|e| format!("serialize: {}", e))?;
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, json.as_bytes());
        std::fs::write(&self.file_path(&cred.platform), encoded)
            .map_err(|e| format!("write credential: {}", e))
    }

    pub fn get_credential(&self, platform: &Platform) -> Result<Option<Credential>, String> {
        let path = self.file_path(platform);
        if !path.exists() {
            return Ok(None);
        }
        let encoded = std::fs::read_to_string(&path)
            .map_err(|e| format!("read credential: {}", e))?;
        let decoded = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            encoded.trim(),
        )
        .map_err(|e| format!("decode credential: {}", e))?;
        let cred: Credential = serde_json::from_slice(&decoded)
            .map_err(|e| format!("deserialize credential: {}", e))?;
        Ok(Some(cred))
    }

    pub fn remove_credential(&self, platform: &Platform) -> Result<(), String> {
        let path = self.file_path(platform);
        if path.exists() {
            std::fs::remove_file(&path).map_err(|e| format!("remove credential: {}", e))
        } else {
            Ok(())
        }
    }

    pub fn has_credential(&self, platform: &Platform) -> bool {
        self.file_path(platform).exists()
    }

    pub fn list_platforms(&self) -> Vec<Platform> {
        let mut platforms = Vec::new();
        for p in &[Platform::Instagram, Platform::Twitter, Platform::LinkedIn] {
            if self.has_credential(p) {
                platforms.push(p.clone());
            }
        }
        platforms
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_roundtrip() {
        let store = SecureStore::new();
        let cred = Credential {
            platform: Platform::Twitter,
            session_token: "test_token_123".into(),
            user_id: "user_42".into(),
        };

        store.store_credential(&cred).unwrap();
        let retrieved = store.get_credential(&Platform::Twitter).unwrap().unwrap();

        assert_eq!(retrieved.platform, cred.platform);
        assert_eq!(retrieved.session_token, cred.session_token);
        assert_eq!(retrieved.user_id, cred.user_id);

        store.remove_credential(&Platform::Twitter).unwrap();
        assert!(store.get_credential(&Platform::Twitter).unwrap().is_none());
    }

    #[test]
    fn test_missing_credential() {
        let store = SecureStore::new();
        store.remove_credential(&Platform::LinkedIn).ok();
        let result = store.get_credential(&Platform::LinkedIn).unwrap();
        assert!(result.is_none());
    }
}
