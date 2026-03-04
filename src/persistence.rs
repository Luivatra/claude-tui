use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub name: String,
    pub directory: PathBuf,
    pub claude_cmd: String,
    #[serde(default)]
    pub conversation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PersistedState {
    pub sessions: Vec<SessionState>,
    pub active_index: usize,
}

impl PersistedState {
    pub fn load() -> Result<Option<Self>> {
        let path = Self::state_path();
        if !path.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&path)?;
        let state: PersistedState = serde_json::from_str(&content)?;

        if state.sessions.is_empty() {
            return Ok(None);
        }

        Ok(Some(state))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::state_path();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn clear() -> Result<()> {
        let path = Self::state_path();
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    fn state_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("claude-tui")
            .join("sessions.json")
    }
}
