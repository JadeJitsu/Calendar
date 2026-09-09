//! The Settings dialog: application-level preferences (week numbers,
//! background CalDAV sync interval).

use cosmic::iced::Length;
use cosmic::widget::{button, column, container, dialog, radio, row};
use cosmic::{widget, Element};

use crate::dialogs::ActiveDialog;
use crate::fl;
use crate::message::Message;

/// The background sync interval choices offered in the dialog, in seconds.
/// The first entry is the default (15 minutes).
pub const SYNC_INTERVAL_OPTIONS: [u64; 4] = [300, 900, 1800, 3600];

/// Localized label for a sync interval (seconds).
fn interval_label(secs: u64) -> String {
    match secs {
        300 => fl!("settings-sync-interval-5min"),
        900 => fl!("settings-sync-interval-15min"),
        1800 => fl!("settings-sync-interval-30min"),
        3600 => fl!("settings-sync-interval-1hour"),
        _ => fl!("settings-sync-interval-15min"),
    }
    .to_string()
}

/// Render the settings dialog. Takes the active dialog state, which should be
/// the `Settings` variant.
pub fn render_settings_dialog(active_dialog: &ActiveDialog) -> Element<'_, Message> {
    let (show_week_numbers, sync_interval_secs, close_to_tray) = match active_dialog {
        ActiveDialog::Settings {
            show_week_numbers,
            sync_interval_secs,
            close_to_tray,
        } => (*show_week_numbers, *sync_interval_secs, *close_to_tray),
        _ => return widget::text("").into(), // Should not happen
    };

    // Week numbers checkbox
    let toggled = !show_week_numbers;
    let week_numbers_control = row([])
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center)
        .push(
            widget::checkbox(show_week_numbers).label("")
                .on_toggle(move |_| Message::SettingsWeekNumbersToggled(toggled)),
        )
        .push(widget::text(fl!("settings-week-numbers")));

    // Close-to-tray checkbox
    let close_to_tray_toggled = !close_to_tray;
    let close_to_tray_control = row([])
        .spacing(8)
        .align_y(cosmic::iced::Alignment::Center)
        .push(
            widget::checkbox(close_to_tray).label("")
                .on_toggle(move |_| Message::SettingsCloseToTrayToggled(close_to_tray_toggled)),
        )
        .push(widget::text(fl!("settings-close-to-tray")));

    // Background sync interval radio group
    let mut interval_control = column([]).spacing(8);
    interval_control = interval_control.push(widget::text(fl!("settings-sync-interval")));
    for secs in SYNC_INTERVAL_OPTIONS {
        let is_selected = Some(secs) == Some(sync_interval_secs);
        let label = interval_label(secs);
        let radio_btn = radio(
            widget::text(label),
            secs,
            if is_selected { Some(sync_interval_secs) } else { None },
            |secs| Message::SettingsSyncIntervalSelected(secs),
        );
        interval_control = interval_control.push(container(radio_btn).padding([0, 0, 0, 16]));
    }

    dialog()
        .title(fl!("settings-dialog-title"))
        .control(week_numbers_control)
        .control(close_to_tray_control)
        .control(interval_control)
        .secondary_action(button::text(fl!("button-cancel")).on_press(Message::CancelSettings))
        .primary_action(button::suggested(fl!("button-save")).on_press(Message::ConfirmSettings))
        .width(Length::Fixed(420.0))
        .into()
}
