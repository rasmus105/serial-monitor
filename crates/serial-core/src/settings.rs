//! Settings persistence utilities
//!
//! Provides platform-aware configuration directory detection and generic
//! load/save functions for TOML-based settings files.
//!
//! # Design
//!
//! This module provides primitives for frontends to build their own settings
//! persistence. The core library does NOT define what settings should exist -
//! that's up to each frontend. This keeps the core frontend-agnostic.
//!
//! # Example
//!
//! ```ignore
//! use serde::{Deserialize, Serialize};
//! use serial_core::settings;
//!
//! #[derive(Serialize, Deserialize, Default)]
//! struct MySettings {
//!     encoding: serial_core::Encoding,
//!     show_timestamps: bool,
//! }
//!
//! // Load settings (returns default if file doesn't exist)
//! let config_dir = settings::config_directory("my-app");
//! let settings: MySettings = settings::load(&config_dir, "settings.toml")
//!     .unwrap_or_default();
//!
//! // Save settings
//! settings::save(&config_dir, "settings.toml", &settings)?;
//! ```

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Serialize, de::DeserializeOwned};

/// Get the platform-appropriate configuration directory for the given app name.
///
/// Creates the directory if it doesn't exist.
///
/// Platform paths:
/// - Linux: `~/.config/{app_name}/`
/// - macOS: `~/Library/Application Support/{app_name}/`
/// - Windows: `%APPDATA%/{app_name}/`
///
/// Falls back to `./{app_name}/` in the current directory if the platform
/// config directory cannot be determined.
pub fn config_directory(app_name: &str) -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(app_name)
}

/// Get the platform-appropriate cache directory for the given app name.
///
/// Platform paths:
/// - Linux: `~/.cache/{app_name}/`
/// - macOS: `~/Library/Caches/{app_name}/`
/// - Windows: `%LOCALAPPDATA%/{app_name}/`
///
/// Falls back to `./{app_name}/` in the current directory if the platform
/// cache directory cannot be determined.
pub fn cache_directory(app_name: &str) -> PathBuf {
    let base = dirs::cache_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(app_name)
}

/// Error type for settings operations.
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// IO error reading/writing file
    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    /// TOML parsing error
    #[error("Failed to parse settings: {0}")]
    Parse(#[from] toml::de::Error),

    /// TOML serialization error
    #[error("Failed to serialize settings: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Load settings from a TOML file.
///
/// Returns `Ok(None)` if the file doesn't exist.
/// Returns `Err` if the file exists but cannot be read or parsed.
///
/// # Arguments
///
/// * `directory` - The directory containing the settings file
/// * `filename` - The name of the settings file (e.g., "settings.toml")
pub fn load<T: DeserializeOwned>(
    directory: &Path,
    filename: &str,
) -> Result<Option<T>, SettingsError> {
    let path = directory.join(filename);

    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)?;
    let settings = toml::from_str(&content)?;
    Ok(Some(settings))
}

/// Load settings from a TOML file, returning default if not found.
///
/// This is a convenience wrapper around [`load`] that returns `T::default()`
/// if the file doesn't exist.
///
/// # Arguments
///
/// * `directory` - The directory containing the settings file
/// * `filename` - The name of the settings file (e.g., "settings.toml")
pub fn load_or_default<T: DeserializeOwned + Default>(
    directory: &Path,
    filename: &str,
) -> Result<T, SettingsError> {
    match load(directory, filename)? {
        Some(settings) => Ok(settings),
        None => Ok(T::default()),
    }
}

/// Save settings to a TOML file.
///
/// Creates the directory and file if they don't exist.
/// Overwrites the file if it already exists.
///
/// # Arguments
///
/// * `directory` - The directory to save the settings file in
/// * `filename` - The name of the settings file (e.g., "settings.toml")
/// * `settings` - The settings to save
pub fn save<T: Serialize>(
    directory: &PathBuf,
    filename: &str,
    settings: &T,
) -> Result<(), SettingsError> {
    // Ensure directory exists
    fs::create_dir_all(directory)?;

    let path = directory.join(filename);
    let content = toml::to_string_pretty(settings)?;
    fs::write(&path, content)?;
    Ok(())
}
