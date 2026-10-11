//! Persisted preferences (JSON in the app config dir) and the Soniox API key
//! (Windows Credential Manager, never written to the settings file).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::hotkeys::Shortcut;

const KEYRING_SERVICE: &str = "Kotev";
const KEYRING_USER: &str = "soniox-api-key";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum InsertMode {
    /// Clipboard + paste shortcut, then restore the clipboard.
    Paste,
    /// Type the text as Unicode key events (slower, leaves the clipboard alone).
    Type,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub app_language: String,
    pub language_hint: String,
    pub insert_automatically: bool,
    pub insert_mode: InsertMode,
    pub selected_microphone: Option<String>,
    pub push_to_talk: Shortcut,
    pub hands_free: Shortcut,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            app_language: "he".into(),
            language_hint: "he".into(),
            insert_automatically: true,
            insert_mode: InsertMode::Paste,
            selected_microphone: None,
            push_to_talk: Shortcut::default_push_to_talk(),
            hands_free: Shortcut::default_hands_free(),
        }
    }
}

impl Settings {
    pub fn load(path: &PathBuf) -> Self {
        let mut settings: Settings = std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        if settings.language_hint != "en" {
            settings.language_hint = "he".into();
        }
        if settings.app_language != "en" {
            settings.app_language = "he".into();
        }
        settings
    }

    pub fn save(&self, path: &PathBuf) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match serde_json::to_string_pretty(self) {
            Ok(text) => {
                if let Err(e) = std::fs::write(path, text) {
                    crate::log::write(&format!("failed to save settings: {e}"));
                }
            }
            Err(e) => crate::log::write(&format!("failed to encode settings: {e}")),
        }
    }
}

fn entry() -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|e| e.to_string())
}

pub fn load_api_key() -> String {
    entry().and_then(|e| e.get_password().map_err(|e| e.to_string())).unwrap_or_default()
}

pub fn save_api_key(key: &str) -> Result<(), String> {
    entry()?.set_password(key).map_err(|e| e.to_string())
}

pub fn remove_api_key() -> Result<(), String> {
    match entry()?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}
