//! Token persistence for --remember-password.
//!
//! Stores refresh tokens in a JSON file so future logins
//! can skip the full authentication flow.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TokenStore {
    /// Map of username → refresh token.
    pub tokens: HashMap<String, String>,
}

impl TokenStore {
    /// Default storage path: `.depotdownloader/tokens.json`
    pub fn default_path() -> PathBuf {
        dirs_path().join("tokens.json")
    }

    /// Load from disk. Returns empty store if file doesn't exist.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save to disk.
    pub fn save(&self, path: &Path) -> Result<(), std::io::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    pub fn get(&self, username: &str) -> Option<&str> {
        self.tokens.get(username).map(|s| s.as_str())
    }

    pub fn set(&mut self, username: String, token: String) {
        self.tokens.insert(username, token);
    }
}

fn dirs_path() -> PathBuf {
    // Use the user's home directory
    if let Some(home) = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
    {
        PathBuf::from(home).join(".depotdownloader")
    } else {
        PathBuf::from(".depotdownloader")
    }
}
