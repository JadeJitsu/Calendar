//! The calendar create/edit dialog and the CalDAV add / calendar-delete
//! / event-delete dialogs.

use cosmic::iced::Length;
use cosmic::widget::{button, column, container, dialog, row, secure_input, text_input};
use cosmic::{widget, Element};

use crate::components::color_picker::{parse_hex_color, QUICK_PICKER_COLORS};
use crate::dialogs::ActiveDialog;
use crate::fl;
use crate::message::Message;
use crate::styles::color_button_style;
use crate::ui_constants::{
    BORDER_WIDTH_HIGHLIGHT, BORDER_WIDTH_SELECTED, border_light, border_selected,
    COLOR_BUTTON_SIZE_SMALL, COLOR_DEFAULT_GRAY, SPACING_COLOR_GRID,
};

/// Render the calendar dialog (Create or Edit mode) using COSMIC dialog widget
/// Takes the active dialog state which should be CalendarCreate or CalendarEdit variant
pub fn render_calendar_dialog(active_dialog: &ActiveDialog) -> Element<'_, Message> {
    // Extract data from active_dialog
    let (is_edit_mode, name, current_color) = match active_dialog {
        ActiveDialog::CalendarCreate { name, color } => (false, name.as_str(), color.as_str()),
        ActiveDialog::CalendarEdit { name, color, .. } => (true, name.as_str(), color.as_str()),
        _ => return widget::text("").into(), // Should not happen
    };

    // Name input field with label
    let name_control = column()
        .spacing(8)
        .push(widget::text(fl!("dialog-calendar-name")))
        .push(
            text_input(fl!("dialog-calendar-name-placeholder"), name)
                .on_input(Message::CalendarDialogNameChanged)
                .on_submit(|_| Message::ConfirmCalendarDialog)
                .width(Length::Fill),
        );

    // Color picker grid using shared color constant
    let mut color_grid = column().spacing(SPACING_COLOR_GRID);

    for row_colors in QUICK_PICKER_COLORS {
        let mut color_row = row().spacing(SPACING_COLOR_GRID);

        for hex in row_colors {
            let color = parse_hex_color(hex).unwrap_or(COLOR_DEFAULT_GRAY);
            let hex_owned = hex.to_string();
            let is_selected = current_color == hex;

            let border_width = if is_selected {
                BORDER_WIDTH_SELECTED
            } else {
                BORDER_WIDTH_HIGHLIGHT
            };

            let color_button = button::custom(
                container(widget::text(""))
                    .width(COLOR_BUTTON_SIZE_SMALL)
                    .height(COLOR_BUTTON_SIZE_SMALL)
                    .style(move |theme: &cosmic::Theme| {
                        let border_color = if is_selected {
                            border_selected(theme)
                        } else {
                            border_light(theme)
                        };
                        color_button_style(color, COLOR_BUTTON_SIZE_SMALL, border_width, border_color)
                    }),
            )
            .on_press(Message::CalendarDialogColorChanged(hex_owned))
            .padding(0);

            color_row = color_row.push(color_button);
        }

        color_grid = color_grid.push(color_row);
    }

    // Color control with label
    let color_control = column()
        .spacing(8)
        .push(widget::text(fl!("dialog-calendar-color")))
        .push(color_grid);

    // Dialog title changes based on mode
    let title = if is_edit_mode {
        fl!("dialog-edit-calendar-title")
    } else {
        fl!("dialog-new-calendar-title")
    };

    // Primary action button text changes based on mode
    let primary_btn = if is_edit_mode {
        button::suggested(fl!("button-save")).on_press(Message::ConfirmCalendarDialog)
    } else {
        button::suggested(fl!("button-create")).on_press(Message::ConfirmCalendarDialog)
    };

    // Use COSMIC's dialog widget with controls
    dialog()
        .title(title)
        .control(name_control)
        .control(color_control)
        .secondary_action(
            button::text(fl!("button-cancel")).on_press(Message::CancelCalendarDialog),
        )
        .primary_action(primary_btn)
        .width(Length::Fixed(350.0))
        .into()
}

/// Render the add CalDAV account dialog (server URL + username + password).
/// Takes the active dialog state which should be the AddCalDav variant.
pub fn render_add_caldav_dialog(active_dialog: &ActiveDialog) -> Element<'_, Message> {
    // Extract the three fields from the AddCalDav variant
    let (url, username, password) = match active_dialog {
        ActiveDialog::AddCalDav {
            url,
            username,
            password,
        } => (url.as_str(), username.as_str(), password.as_str()),
        _ => return widget::text("").into(), // Should not happen
    };

    // Server URL input (must be https://)
    let url_control = column()
        .spacing(8)
        .push(widget::text(fl!("dialog-add-caldav-url")))
        .push(
            text_input(fl!("dialog-add-caldav-url-placeholder"), url)
                .on_input(Message::CalDavDialogUrlChanged)
                .on_submit(|_| Message::ConfirmAddCalDav)
                .width(Length::Fill),
        );

    // Username input
    let user_control = column()
        .spacing(8)
        .push(widget::text(fl!("dialog-add-caldav-username")))
        .push(
            text_input(fl!("dialog-add-caldav-username-placeholder"), username)
                .on_input(Message::CalDavDialogUserChanged)
                .on_submit(|_| Message::ConfirmAddCalDav)
                .width(Length::Fill),
        );

    // Password input (masked)
    let password_control = column()
        .spacing(8)
        .push(widget::text(fl!("dialog-add-caldav-password")))
        .push(
            secure_input(fl!("dialog-add-caldav-password-placeholder"), password, None, true)
                .on_input(Message::CalDavDialogPasswordChanged)
                .on_submit(|_| Message::ConfirmAddCalDav)
                .width(Length::Fill),
        );

    // Note about HTTPS-only + keyring storage
    let note = widget::text(fl!("dialog-add-caldav-https-note")).size(12);

    // Connect button is only enabled when URL and username are non-empty.
    // (The password may legitimately be empty for servers that use other
    // auth, but our Basic-auth scope requires it — the confirm handler
    // validates and the button stays enabled so the user gets a clear
    // error rather than a mysteriously-dead button.)
    let primary_btn = button::suggested(fl!("button-connect")).on_press(Message::ConfirmAddCalDav);

    dialog()
        .title(fl!("dialog-add-caldav-title"))
        .icon(widget::icon::from_name("emblem-web-symbolic").size(64))
        .control(url_control)
        .control(user_control)
        .control(password_control)
        .control(note)
        .secondary_action(
            button::text(fl!("button-cancel")).on_press(Message::CancelAddCalDav),
        )
        .primary_action(primary_btn)
        .width(Length::Fixed(420.0))
        .into()
}

/// Render the delete calendar confirmation dialog using COSMIC dialog widget
/// Takes the active dialog state which should be CalendarDelete variant
pub fn render_delete_calendar_dialog(active_dialog: &ActiveDialog) -> Element<'_, Message> {
    // Extract calendar name from active_dialog
    let calendar_name = match active_dialog {
        ActiveDialog::CalendarDelete { calendar_name, .. } => calendar_name.as_str(),
        _ => return widget::text("").into(), // Should not happen
    };

    // Use COSMIC's dialog widget with proper styling
    dialog()
        .title(fl!("dialog-delete-calendar-title"))
        .icon(widget::icon::from_name("dialog-warning-symbolic").size(64))
        .body(fl!(
            "dialog-delete-calendar-message",
            name = calendar_name.to_string()
        ))
        .secondary_action(
            button::text(fl!("button-cancel")).on_press(Message::CancelDeleteCalendar),
        )
        .primary_action(
            button::destructive(fl!("button-delete")).on_press(Message::ConfirmDeleteCalendar),
        )
        .width(Length::Fixed(400.0))
        .into()
}

/// Render the delete event confirmation dialog using COSMIC dialog widget
/// Takes the active dialog state which should be EventDelete variant
pub fn render_delete_event_dialog(active_dialog: &ActiveDialog) -> Element<'_, Message> {
    // Extract event data from active_dialog
    let (event_name, is_recurring) = match active_dialog {
        ActiveDialog::EventDelete { event_name, is_recurring, .. } => (event_name.as_str(), *is_recurring),
        _ => return widget::text("").into(), // Should not happen
    };

    if is_recurring {
        // For recurring events, show three buttons: Cancel, Delete This One, Delete All
        // The body explains this is a recurring event
        let body_message = fl!("dialog-delete-event-recurring-message");

        // Create a custom button row with three buttons - wrap to allow buttons to adapt
        let button_row = row()
            .spacing(8)
            .push(
                button::text(fl!("button-cancel"))
                    .on_press(Message::CancelDeleteEvent)
                    .width(Length::Shrink),
            )
            .push(
                button::standard(fl!("button-delete-this-occurrence"))
                    .on_press(Message::DeleteSingleOccurrence)
                    .width(Length::Shrink),
            )
            .push(
                button::destructive(fl!("button-delete-all-occurrences"))
                    .on_press(Message::ConfirmDeleteEvent)
                    .width(Length::Shrink),
            );

        // Build dialog content with event name as title context
        let content = column()
            .spacing(16)
            .push(widget::text::title4(event_name))
            .push(widget::text(body_message))
            .push(button_row);

        // Use a simple dialog without the standard primary/secondary buttons
        dialog()
            .title(fl!("dialog-delete-event-title"))
            .icon(widget::icon::from_name("dialog-warning-symbolic").size(64))
            .control(content)
            .width(Length::Fixed(400.0))
            .into()
    } else {
        // For non-recurring events, show simple confirmation with Cancel and Delete
        let body_message = fl!("dialog-delete-event-message", name = event_name.to_string());

        dialog()
            .title(fl!("dialog-delete-event-title"))
            .icon(widget::icon::from_name("dialog-warning-symbolic").size(64))
            .body(body_message)
            .secondary_action(
                button::text(fl!("button-cancel")).on_press(Message::CancelDeleteEvent),
            )
            .primary_action(
                button::destructive(fl!("button-delete")).on_press(Message::ConfirmDeleteEvent),
            )
            .width(Length::Fixed(400.0))
            .into()
    }
}
