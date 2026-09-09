//! Color constants for consistent theming across the application.

use cosmic::iced::Color;

/// Neutral gray used as the default event/foreground color.
pub const COLOR_DEFAULT_GRAY: Color = Color::from_rgb(107.0/255.0, 114.0/255.0, 128.0/255.0);
/// Faint black border for unselected cells/elements.
pub const COLOR_BORDER_LIGHT: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.2);
/// Solid black border for selected cells/elements.
pub const COLOR_BORDER_SELECTED: Color = Color::from_rgb(0.0, 0.0, 0.0);
/// Subtle border between individual day cells in the month grid.
pub const COLOR_DAY_CELL_BORDER: Color = Color::from_rgba(0.5, 0.5, 0.5, 0.2);
/// Subtle gray tint behind weekend day cells.
pub const COLOR_WEEKEND_BACKGROUND: Color = Color::from_rgba(0.5, 0.5, 0.5, 0.05);

/// Blue color for "today" indicator circle - consistent across all themes
pub const COLOR_TODAY_BLUE: Color = Color::from_rgb(0.0, 122.0/255.0, 255.0/255.0); // #007AFF

/// Red color for the current time indicator line in week/day views
pub const COLOR_CURRENT_TIME: Color = Color::from_rgb(1.0, 59.0/255.0, 48.0/255.0); // #FF3B30

/// Light blue background for time slot selection in week/day views
/// Semi-transparent so it overlays naturally on cells
#[allow(dead_code)] // Reserved for future time slot selection feature
pub const COLOR_SELECTION_BACKGROUND: Color = Color::from_rgba(0.0, 122.0/255.0, 255.0/255.0, 0.2); // #007AFF @ 20%
