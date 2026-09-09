//! Calendar configuration: the `CalendarConfig` record and the
//! `CalendarManagerConfig` that loads/saves the full calendar list to disk.

use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::PathBuf;

/// Configuration for a calendar source
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarConfig {
    pub id: String,
    pub name: String,
    pub color: String,
    pub enabled: bool,
    pub calendar_type: String,
    /// Per-calendar CalDAV collection URL (e.g. `https://host/remote.php/dav/calendars/user/work/`).
    /// Only set for `calendar_type == "caldav"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_url: Option<String>,
    /// CalDAV username. Only set for `calendar_type == "caldav"`.
    /// The password is NOT stored here — it lives only in the system keyring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
}

/// Manager configuration that stores all calendar settings
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CalendarManagerConfig {
    pub calendars: Vec<CalendarConfig>,
}

impl CalendarManagerConfig {
    /// Load configuration from disk
    pub fn load() -> Result<Self, io::Error> {
        let path = Self::config_path();

        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(&path)?;
        let config: CalendarManagerConfig = serde_json::from_str(&contents)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(config)
    }

    /// Save configuration to disk
    pub fn save(&self) -> Result<(), io::Error> {
        let path = Self::config_path();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let json = serde_json::to_string_pretty(self)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(&path, json)?;
        Ok(())
    }

    /// Get the configuration file path
    fn config_path() -> PathBuf {
        let mut path = dirs::config_local_dir().unwrap_or_else(|| PathBuf::from("."));
        path.push("sol-calendar");
        path.push("calendars.json");
        path
    }

    /// Update or add a calendar configuration
    pub fn update_calendar(&mut self, config: CalendarConfig) {
        if let Some(existing) = self.calendars.iter_mut().find(|c| c.id == config.id) {
            *existing = config;
        } else {
            self.calendars.push(config);
        }
    }

    /// Get a calendar configuration by ID
    pub fn get_calendar(&self, id: &str) -> Option<&CalendarConfig> {
        self.calendars.iter().find(|c| c.id == id)
    }

    /// Remove a calendar configuration
    pub fn remove_calendar(&mut self, id: &str) -> bool {
        if let Some(index) = self.calendars.iter().position(|c| c.id == id) {
            self.calendars.remove(index);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caldav_config() -> CalendarConfig {
        CalendarConfig {
            id: "caldav-1".to_string(),
            name: "Work".to_string(),
            color: "#ff0000".to_string(),
            enabled: true,
            calendar_type: "caldav".to_string(),
            server_url: Some("https://example.com/remote.php/dav/calendars/user/work/".to_string()),
            username: Some("user".to_string()),
        }
    }

    #[test]
    fn test_caldav_config_round_trip() {
        let config = caldav_config();
        let json = serde_json::to_string(&config).expect("serialize");
        let back: CalendarConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, config);
    }

    /// The password must never be serializable — the struct has no password
    /// field, and the serialized JSON of a caldav entry must not contain one.
    #[test]
    fn test_caldav_config_json_has_no_password() {
        let json = serde_json::to_string(&caldav_config()).expect("serialize");
        assert!(!json.to_lowercase().contains("password"));
        assert!(json.contains("server_url"));
        assert!(json.contains("username"));
    }

    /// A local (non-caldav) entry omits the optional fields entirely
    /// (`skip_serializing_if`), and still deserializes with them as `None`.
    #[test]
    fn test_local_config_omits_optional_fields() {
        let config = CalendarConfig {
            id: "local-1".to_string(),
            name: "Personal".to_string(),
            color: "#00ff00".to_string(),
            enabled: true,
            calendar_type: "local".to_string(),
            server_url: None,
            username: None,
        };
        let json = serde_json::to_string(&config).expect("serialize");
        assert!(!json.contains("server_url"));
        assert!(!json.contains("username"));
        let back: CalendarConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.server_url, None);
        assert_eq!(back.username, None);
    }
}
