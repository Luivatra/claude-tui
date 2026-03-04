use anyhow::Result;
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "claude-tui")]
#[command(about = "Terminal multiplexer for Claude Code sessions")]
pub struct Args {
    /// Default Claude command to use
    #[arg(long, default_value = "claude")]
    pub claude_cmd: String,

    /// Working directory for new sessions
    #[arg(short, long)]
    pub directory: Option<PathBuf>,

    /// Config file path
    #[arg(long)]
    pub config: Option<PathBuf>,

    /// Claude config directory (sets CLAUDE_CONFIG_DIR for sessions)
    #[arg(long)]
    pub claude_config_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_claude_cmd")]
    pub default_claude_cmd: String,

    #[serde(default)]
    pub aliases: HashMap<String, String>,

    #[serde(skip)]
    pub claude_config_dir: Option<PathBuf>,
}

fn default_claude_cmd() -> String {
    "claude".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_claude_cmd: default_claude_cmd(),
            aliases: HashMap::new(),
            claude_config_dir: None,
        }
    }
}

impl Config {
    pub fn load(path: Option<&PathBuf>) -> Result<Self> {
        let config_path = path.cloned().unwrap_or_else(|| {
            dirs::config_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("claude-tui")
                .join("config.toml")
        });

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)?;
            Ok(config)
        } else {
            Ok(Config::default())
        }
    }

    pub fn resolve_alias(&self, name: &str) -> String {
        self.aliases
            .get(name)
            .cloned()
            .unwrap_or_else(|| name.to_string())
    }

    pub fn get_claude_cmd(&self, override_cmd: Option<&str>) -> String {
        override_cmd
            .map(|s| self.resolve_alias(s))
            .unwrap_or_else(|| self.default_claude_cmd.clone())
    }
}
