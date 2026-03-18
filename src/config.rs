use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// The in-memory representation of a penv config file.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Config {
    /// Ordered map of variable name -> value.
    #[serde(default)]
    pub vars: BTreeMap<String, String>,
}

impl Config {
    /// Load a config from a YAML file.  Returns an empty config if the file
    /// does not exist yet.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)?;
        let cfg: Self = serde_yaml::from_str(&content)?;
        Ok(cfg)
    }

    /// Persist the config to a YAML file, creating parent directories as
    /// needed.
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let yaml = serde_yaml::to_string(self)?;
        fs::write(path, yaml)?;
        Ok(())
    }

    /// Return the canonical path of the active config:
    /// `~/.local/penv/current.yaml`.
    pub fn current_path() -> anyhow::Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".local").join("penv").join("current.yaml"))
    }

    /// Return the path for a named profile:
    /// `~/.local/penv/<name>.yaml`.
    pub fn profile_path(name: &str) -> anyhow::Result<PathBuf> {
        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine home directory"))?;
        Ok(home.join(".local").join("penv").join(format!("{name}.yaml")))
    }
}
