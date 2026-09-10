//! The `CalendarSource` trait and the `CalendarInfo`/`CalendarType`
//! types that unify local and CalDAV calendars behind one interface.

use crate::caldav::CalendarEvent;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::Debug;

/// Type of calendar source
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarType {
    Local,
    CalDav,
    Google,
    Outlook,
    ICloud,
    Other,
}

impl CalendarType {
    /// Human-readable label for this calendar type (e.g. "CalDAV").
    #[allow(dead_code)] // Reserved for future calendar type display
    pub fn as_str(&self) -> &str {
        match self {
            CalendarType::Local => "Local",
            CalendarType::CalDav => "CalDAV",
            CalendarType::Google => "Google Calendar",
            CalendarType::Outlook => "Outlook",
            CalendarType::ICloud => "iCloud",
            CalendarType::Other => "Other",
        }
    }

    /// The value persisted in `CalendarConfig.calendar_type`.
    ///
    /// Must stay in sync with the load-side check in
    /// `CalendarManager::with_defaults` — both write and read go through
    /// this method so the config round-trips. (Regression: the load path
    /// once compared against a hardcoded lowercase `"caldav"` while the
    /// save path wrote the Debug form `"CalDav"`, so saved CalDAV
    /// calendars silently loaded as local calendars and never synced.)
    pub fn as_config(&self) -> &'static str {
        match self {
            CalendarType::Local => "Local",
            CalendarType::CalDav => "CalDav",
            CalendarType::Google => "Google",
            CalendarType::Outlook => "Outlook",
            CalendarType::ICloud => "ICloud",
            CalendarType::Other => "Other",
        }
    }
}

/// Metadata about a calendar source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarInfo {
    /// Unique identifier for this calendar
    pub id: String,
    /// Display name of the calendar
    pub name: String,
    /// Type of calendar source
    pub calendar_type: CalendarType,
    /// Color for displaying events (hex format: "#RRGGBB")
    pub color: String,
    /// Description of the calendar
    pub description: Option<String>,
    /// Whether the calendar is currently enabled/visible
    pub enabled: bool,
}

impl CalendarInfo {
    /// Build a calendar with a type-appropriate default color, no
    /// description, and `enabled: true`.
    pub fn new(id: String, name: String, calendar_type: CalendarType) -> Self {
        CalendarInfo {
            id,
            name,
            calendar_type,
            color: Self::default_color_for_type(calendar_type),
            description: None,
            enabled: true,
        }
    }

    fn default_color_for_type(calendar_type: CalendarType) -> String {
        match calendar_type {
            CalendarType::Local => "#3B82F6".to_string(),   // blue
            CalendarType::CalDav => "#8B5CF6".to_string(),  // purple
            CalendarType::Google => "#EA4335".to_string(),  // google red
            CalendarType::Outlook => "#0078D4".to_string(), // outlook blue
            CalendarType::ICloud => "#007AFF".to_string(),  //
            CalendarType::Other => "#6B7280".to_string(),   // gray
        }
    }
}

/// Trait that all calendar sources must implement
pub trait CalendarSource: Debug + Send {
    /// Get metadata about this calendar
    fn info(&self) -> &CalendarInfo;

    /// Get mutable reference to calendar info
    fn info_mut(&mut self) -> &mut CalendarInfo;

    /// Check if this calendar is enabled
    fn is_enabled(&self) -> bool {
        self.info().enabled
    }

    /// Enable or disable this calendar
    fn set_enabled(&mut self, enabled: bool) {
        self.info_mut().enabled = enabled;
    }

    /// Fetch all events from this calendar source
    fn fetch_events(&self) -> Result<Vec<CalendarEvent>, Box<dyn Error>>;

    /// Add a new event to this calendar
    fn add_event(&mut self, event: CalendarEvent) -> Result<(), Box<dyn Error>>;

    /// Update an existing event
    #[allow(dead_code)] // Part of trait API for future use
    fn update_event(&mut self, event: CalendarEvent) -> Result<(), Box<dyn Error>>;

    /// Delete an event by UID
    fn delete_event(&mut self, uid: &str) -> Result<(), Box<dyn Error>>;

    /// Sync with the remote source (for remote calendars)
    /// For local calendars, this might just save to disk
    fn sync(&mut self) -> Result<(), Box<dyn Error>>;

    /// Check if this calendar supports read operations
    #[allow(dead_code)] // Part of trait API for future use
    fn supports_read(&self) -> bool {
        true
    }

    /// Check if this calendar supports write operations
    #[allow(dead_code)] // Part of trait API for future use
    fn supports_write(&self) -> bool {
        true
    }

    /// Remote connection config for persistence, if this is a remote source.
    /// Returns `(server_url, username)` for CalDAV; `None` for local sources.
    /// The password is intentionally NOT part of this — it lives in the keyring.
    fn remote_config(&self) -> Option<(String, String)> {
        None
    }

    /// Downcast to the concrete source type (used to apply CalDAV sync
    /// results to a `CalDavCalendar` cache).
    fn as_any(&mut self) -> &mut dyn std::any::Any;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `CalendarType::CalDav.as_config()` must match what `save_config`
    /// writes to disk, so a saved config round-trips back through the
    /// `calendar_type == ...` check in `CalendarManager::with_defaults`.
    /// Regression: the load path compared against the lowercase literal
    /// `"caldav"` while `save_config` wrote the Debug form `"CalDav"`, so
    /// saved CalDAV calendars silently loaded as local calendars and never
    /// synced.
    #[test]
    fn test_caldav_config_string_round_trips_with_debug_form() {
        assert_eq!(
            CalendarType::CalDav.as_config(),
            format!("{:?}", CalendarType::CalDav)
        );
        assert_eq!(
            CalendarType::Local.as_config(),
            format!("{:?}", CalendarType::Local)
        );
    }
}
