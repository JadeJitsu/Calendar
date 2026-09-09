//! Application-level settings that persist across sessions (JSON file).

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

/// Default background CalDAV sync interval, in seconds (15 minutes).
pub const DEFAULT_BACKGROUND_SYNC_INTERVAL_SECS: u64 = 15 * 60;

fn default_background_sync_interval_secs() -> u64 {
    DEFAULT_BACKGROUND_SYNC_INTERVAL_SECS
}

/// Application-level settings that persist across sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub show_week_numbers: bool,
    /// How often background CalDAV sync runs, in seconds. Defaults to 15
    /// minutes; old config files without this field load with the default.
    #[serde(default = "default_background_sync_interval_secs")]
    pub background_sync_interval_secs: u64,
    /// Whether closing the window minimizes to the system tray (and keeps the
    /// app running in the background) instead of exiting. Defaults to false;
    /// old config files without this field load as false.
    #[serde(default)]
    pub close_to_tray: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            show_week_numbers: true, // Show week numbers by default
            background_sync_interval_secs: DEFAULT_BACKGROUND_SYNC_INTERVAL_SECS,
            close_to_tray: false,
        }
    }
}

impl AppSettings {
    /// Load settings from disk
    pub fn load() -> Result<Self, io::Error> {
        let path = Self::settings_path();

        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&path)?;
        let settings: AppSettings = serde_json::from_str(&contents)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(settings)
    }

    /// Save settings to disk
    pub fn save(&self) -> Result<(), io::Error> {
        let path = Self::settings_path();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&path, json)?;
        Ok(())
    }

    /// Get the settings file path
    fn settings_path() -> PathBuf {
        let mut path = dirs::config_local_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("sol-calendar");
        path.push("settings.json");
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_sync_interval_is_15_minutes() {
        assert_eq!(AppSettings::default().background_sync_interval_secs, 900);
    }

    #[test]
    fn serde_default_applies_when_field_missing() {
        // Old config files have no background_sync_interval_secs — serde must
        // fill it with the default rather than failing to parse.
        let json = r#"{"show_week_numbers": true}"#;
        let settings: AppSettings = serde_json::from_str(json).unwrap();
        assert!(settings.show_week_numbers);
        assert_eq!(settings.background_sync_interval_secs, 900);
    }

    #[test]
    fn serde_default_close_to_tray_is_false_when_missing() {
        // Old config files have no close_to_tray — serde must fill it with
        // false rather than failing to parse.
        let json = r#"{"show_week_numbers": true}"#;
        let settings: AppSettings = serde_json::from_str(json).unwrap();
        assert!(!settings.close_to_tray);
    }

    #[test]
    fn serde_round_trips_close_to_tray() {
        let mut settings = AppSettings::default();
        settings.close_to_tray = true;
        let json = serde_json::to_string(&settings).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert!(back.close_to_tray);
    }

    #[test]
    fn serde_round_trips_sync_interval() {
        let mut settings = AppSettings::default();
        settings.background_sync_interval_secs = 300;
        let json = serde_json::to_string(&settings).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.background_sync_interval_secs, 300);
    }
}
