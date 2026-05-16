//! TOML configuration for transcript roots and project aliases.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// User configuration stored at `~/.config/spotter/config.toml` by default.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    /// Transcript roots that Spotter may scan.
    #[serde(default)]
    pub transcript_roots: Vec<PathBuf>,

    /// Project alias definitions.
    #[serde(default)]
    pub projects: Vec<ProjectConfig>,
}

/// A configured project path and its CLI-friendly alias.
#[derive(Debug, Clone, Deserialize, Serialize, Eq, PartialEq)]
pub struct ProjectConfig {
    /// Short name used in command output and `--project` filters.
    pub alias: String,

    /// Canonical project working directory.
    pub path: PathBuf,
}

impl Config {
    /// Read a config file, returning an empty config when the file is absent.
    pub fn read_or_default(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("failed to parse config {}", path.display()))
    }

    /// Write this config to disk.
    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create config dir {}", parent.display()))?;
        }

        let text = toml::to_string_pretty(self).context("failed to encode config")?;
        fs::write(path, text).with_context(|| format!("failed to write config {}", path.display()))
    }

    /// Find the best alias for a transcript working directory.
    pub fn alias_for_cwd(&self, cwd: Option<&str>) -> String {
        let Some(cwd) = cwd else {
            return "unknown".to_string();
        };

        let cwd_path = Path::new(cwd);
        self.projects
            .iter()
            .filter(|project| cwd_path.starts_with(&project.path))
            .max_by_key(|project| project.path.as_os_str().len())
            .map_or_else(
                || {
                    cwd_path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("unknown")
                        .to_string()
                },
                |project| project.alias.clone(),
            )
    }

    /// Add or replace a project alias.
    pub fn upsert_project(&mut self, alias: String, path: PathBuf) {
        if let Some(project) = self
            .projects
            .iter_mut()
            .find(|project| project.alias == alias)
        {
            project.path = path;
            return;
        }

        self.projects.push(ProjectConfig { alias, path });
        self.projects
            .sort_by(|left, right| left.alias.cmp(&right.alias));
    }

    /// Remove a project alias.
    pub fn remove_project(&mut self, alias: &str) -> bool {
        let before = self.projects.len();
        self.projects.retain(|project| project.alias != alias);
        before != self.projects.len()
    }

    /// Rename a project alias.
    pub fn rename_project(&mut self, old_alias: &str, new_alias: String) -> bool {
        if self
            .projects
            .iter()
            .any(|project| project.alias == new_alias)
        {
            return false;
        }

        if let Some(project) = self
            .projects
            .iter_mut()
            .find(|project| project.alias == old_alias)
        {
            project.alias = new_alias;
            self.projects
                .sort_by(|left, right| left.alias.cmp(&right.alias));
            return true;
        }

        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn config_round_trip_and_alias_management() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join("nested").join("config.toml");

        let mut config = Config::read_or_default(&path).expect("default config");
        assert!(config.projects.is_empty());
        assert_eq!(config.alias_for_cwd(None), "unknown");
        assert_eq!(
            config.alias_for_cwd(Some("/tmp/plain-project")),
            "plain-project"
        );

        config.upsert_project("root".to_string(), PathBuf::from("/work"));
        config.upsert_project("app".to_string(), PathBuf::from("/work/app"));
        config.upsert_project("root".to_string(), PathBuf::from("/workspace"));
        assert_eq!(config.projects[0].alias, "app");
        assert_eq!(config.alias_for_cwd(Some("/work/app/crate")), "app");
        assert_eq!(config.alias_for_cwd(Some("/workspace/tool")), "root");

        assert!(!config.rename_project("app", "root".to_string()));
        assert!(!config.rename_project("missing", "other".to_string()));
        assert!(config.rename_project("app", "renamed".to_string()));
        assert!(config.remove_project("renamed"));
        assert!(!config.remove_project("renamed"));

        config.write(&path).expect("write config");
        let loaded = Config::read_or_default(&path).expect("read config");
        assert_eq!(loaded.projects, config.projects);
    }
}
