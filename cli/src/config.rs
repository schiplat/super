use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CliConfig {
    pub server_url: String,
    #[serde(default)]
    pub auth_token: Option<String>,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            server_url: "http://127.0.0.1:9002".to_string(),
            auth_token: None,
        }
    }
}

impl CliConfig {
    /// Config file path: ~/.super/cli.json
    fn path() -> PathBuf {
        let home = dirs::home_dir().expect("Cannot find home directory");
        home.join(".super").join("cli.json")
    }

    /// Load config; return defaults if missing
    pub fn load() -> Self {
        let path = Self::path();
        if path.exists()
            && let Ok(content) = fs::read_to_string(&path)
        {
            return serde_json::from_str(&content).unwrap_or_default();
        }
        Self::default()
    }

    /// Whether a persisted CLI config file exists — i.e. the user has made a
    /// deliberate endpoint choice (via `super login`), which we never
    /// auto-override with socket discovery.
    pub fn exists() -> bool {
        Self::path().exists()
    }

    /// Save config to disk (atomic write + permission control)
    pub fn save(&self) -> anyhow::Result<()> {
        let path = Self::path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        fs::write(&path, content)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&path)?.permissions();
            perms.set_mode(0o600);
            fs::set_permissions(&path, perms)?;
        }
        Ok(())
    }
}
