//! Color constants for consistent theming across the application.

use cosmic::cosmic_theme::palette::WithAlpha;
use cosmic::iced::Color;

/// Neutral gray used as the default event/foreground color.
pub const COLOR_DEFAULT_GRAY: Color = Color::from_rgb(107.0 / 255.0, 114.0 / 255.0, 128.0 / 255.0);

/// Blue color for "today" indicator circle - consistent across all themes
pub const COLOR_TODAY_BLUE: Color = Color::from_rgb(0.0, 122.0 / 255.0, 1.0); // #007AFF

/// Red color for the current time indicator line in week/day views
pub const COLOR_CURRENT_TIME: Color = Color::from_rgb(1.0, 59.0 / 255.0, 48.0 / 255.0); // #FF3B30

/// Light blue background for time slot selection in week/day views
/// Semi-transparent so it overlays naturally on cells
#[allow(dead_code)] // Reserved for future time slot selection feature
pub const COLOR_SELECTION_BACKGROUND: Color = Color::from_rgba(0.0, 122.0 / 255.0, 1.0, 0.2); // #007AFF @ 20%

/// Subtle border between day cells / hour rows, derived from the active
/// theme so it stays visible in both light and dark mode. Uses the theme's
/// small-widget divider token (a faint neutral that inverts per theme).
pub fn day_cell_border(theme: &cosmic::Theme) -> Color {
    theme.cosmic().small_widget_divider().into()
}

/// Faint border for unselected cells/elements (e.g. color swatches).
/// Derived from the active theme's small-widget divider token.
pub fn border_light(theme: &cosmic::Theme) -> Color {
    theme.cosmic().small_widget_divider().into()
}

/// Solid border for selected cells/elements. Uses the theme's on-background
/// (primary text) color so it reads as a strong, theme-correct outline.
pub fn border_selected(theme: &cosmic::Theme) -> Color {
    theme.cosmic().on_bg_color().into()
}

/// Subtle tint behind weekend day cells, derived from the active theme's
/// on-background color at low opacity so it works on both light and dark
/// backgrounds.
pub fn weekend_tint(theme: &cosmic::Theme) -> Color {
    theme.cosmic().on_bg_color().with_alpha(0.05).into()
}

/// Dimmed text color for adjacent-month day cells, derived from the active
/// theme's on-background color at reduced opacity.
pub fn adjacent_month_text(theme: &cosmic::Theme) -> Color {
    theme.cosmic().on_bg_color().with_alpha(0.5).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dark() -> cosmic::Theme {
        cosmic::Theme::dark()
    }
    fn light() -> cosmic::Theme {
        cosmic::Theme::light()
    }

    #[test]
    fn day_cell_border_flips_between_themes() {
        // The subtle divider must be a different (visible) color in each theme,
        // not a fixed black/gray that vanishes in one of them.
        let d = day_cell_border(&dark());
        let l = day_cell_border(&light());
        assert_ne!(d, l, "day cell border must differ between dark and light");
        // Both should be semi-transparent dividers, not opaque.
        assert!(d.a < 1.0);
        assert!(l.a < 1.0);
    }

    #[test]
    fn border_selected_flips_between_themes() {
        let d = border_selected(&dark());
        let l = border_selected(&light());
        assert_ne!(d, l, "selected border must differ between dark and light");
    }

    #[test]
    fn border_light_flips_between_themes() {
        let d = border_light(&dark());
        let l = border_light(&light());
        assert_ne!(d, l, "light border must differ between dark and light");
    }

    #[test]
    fn weekend_tint_flips_between_themes() {
        let d = weekend_tint(&dark());
        let l = weekend_tint(&light());
        assert_ne!(d, l, "weekend tint must differ between dark and light");
        // Tint stays faint in both themes.
        assert!(d.a < 0.2);
        assert!(l.a < 0.2);
    }

    #[test]
    fn adjacent_month_text_flips_between_themes() {
        let d = adjacent_month_text(&dark());
        let l = adjacent_month_text(&light());
        assert_ne!(
            d, l,
            "adjacent-month text must differ between dark and light"
        );
    }
}
