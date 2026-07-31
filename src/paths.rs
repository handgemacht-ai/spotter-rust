//! Filesystem path resolution for Spotter state.

use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use directories::BaseDirs;

/// Environment variable that overrides the global `SQLite` database path.
pub const DB_PATH_ENV: &str = "SPOTTER_DB_PATH";

/// Environment variable that overrides the TOML configuration file path.
pub const CONFIG_PATH_ENV: &str = "SPOTTER_CONFIG_PATH";

/// Environment variable that overrides the embedding model cache directory.
pub const MODEL_DIR_ENV: &str = "SPOTTER_MODEL_DIR";

/// Resolve the `SQLite` database path.
pub fn db_path(override_path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(path);
    }

    if let Ok(path) = env::var(DB_PATH_ENV) {
        return Ok(PathBuf::from(path));
    }

    let base = BaseDirs::new().context("could not resolve user data directory")?;
    Ok(base.data_dir().join("spotter").join("spotter.db"))
}

/// Resolve the TOML configuration file path.
pub fn config_path(override_path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(path);
    }

    if let Ok(path) = env::var(CONFIG_PATH_ENV) {
        return Ok(PathBuf::from(path));
    }

    let base = BaseDirs::new().context("could not resolve user config directory")?;
    Ok(base.config_dir().join("spotter").join("config.toml"))
}

/// Resolve the embedding model cache directory.
pub fn model_dir(override_path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = override_path {
        return Ok(path);
    }

    if let Ok(path) = env::var(MODEL_DIR_ENV) {
        return Ok(PathBuf::from(path));
    }

    let base = BaseDirs::new().context("could not resolve user data directory")?;
    Ok(base
        .data_dir()
        .join("spotter")
        .join("models")
        .join(crate::embed::MODEL_ID))
}
