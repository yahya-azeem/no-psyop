use keyring::{Entry, Error};
use crate::types::{Credential, Platform};

const SERVICE_NAME: &str = "no_pysop";

pub struct SecureStore;

impl SecureStore {
    pub fn new() -> Self {
        Self
    }

    fn entry_name(platform: &Platform) -> String {
        format!("credential_{:?}", platform)
    }

    pub fn store_credential(&self, cred: &Credential) -> Result<(), String> {
        let entry = Entry::new(SERVICE_NAME, &Self::entry_name(&cred.platform))
            .map_err(|e| format!("keyring entry: {}", e))?;

        let json = serde_json::to_string(cred).map_err(|e| format!("serialize: {}", e))?;
        entry.set_password(&json).map_err(|e| format!("set password: {}", e))?;

        Ok(())
    }

    pub fn get_credential(&self, platform: &Platform) -> Result<Option<Credential>, String> {
        let entry = Entry::new(SERVICE_NAME, &Self::entry_name(platform))
            .map_err(|e| format!("keyring entry: {}", e))?;

        match entry.get_password() {
            Ok(json) => {
                let cred: Credential = serde_json::from_str(&json)
                    .map_err(|e| format!("deserialize: {}", e))?;
                Ok(Some(cred))
            }
            Err(Error::NoEntry) => Ok(None),
            Err(e) => Err(format!("get password: {}", e)),
        }
    }

    pub fn remove_credential(&self, platform: &Platform) -> Result<(), String> {
        let entry = Entry::new(SERVICE_NAME, &Self::entry_name(platform))
            .map_err(|e| format!("keyring entry: {}", e))?;

        entry.delete_password().map_err(|e| format!("delete password: {}", e))
    }

    pub fn has_credential(&self, platform: &Platform) -> bool {
        self.get_credential(platform).ok().flatten().is_some()
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
    #[ignore = "requires D-Bus secret service (gnome-keyring)"]
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
    #[ignore = "requires D-Bus secret service (gnome-keyring)"]
    fn test_missing_credential() {
        let store = SecureStore::new();
        store.remove_credential(&Platform::LinkedIn).ok();
        let result = store.get_credential(&Platform::LinkedIn).unwrap();
        assert!(result.is_none());
    }
}
