//! The search bar: a text input plus a live results list. Shown above the
//! calendar view when `show_search` is true. Clicking a result opens the
//! event for editing.

use chrono::{Datelike, Utc};
use cosmic::iced::{Alignment, Length};
use cosmic::widget::text_input;
use cosmic::widget::Id;
use cosmic::widget::{button, column, container, row, scrollable, text};
use cosmic::{widget, Element};

use crate::components::parse_color_safe;
use crate::fl;
use crate::message::Message;
use crate::services::SearchResult;
use crate::ui_constants::{PADDING_MEDIUM, SPACING_SMALL};

/// Stable id for the search text input (used to focus it on open).
pub fn search_input_id() -> Id {
    Id::new("search_input")
}

/// Human-readable date for a result row, e.g. "Sep 10, 2026".
fn format_result_date(start: chrono::DateTime<Utc>) -> String {
    let d = start.date_naive();
    format!("{}, {} {}", d.format("%b").to_string(), d.day(), d.year())
}

/// A small filled circle in the calendar's color.
fn color_dot(color: &str) -> Element<'_, Message> {
    let color = parse_color_safe(color);
    container(widget::text(""))
        .width(12.0)
        .height(12.0)
        .style(move |_theme: &cosmic::Theme| container::Style {
            background: Some(cosmic::iced::Background::Color(color)),
            border: cosmic::iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}

/// One clickable result row.
fn render_result_row(r: &SearchResult) -> Element<'_, Message> {
    let mut row = row([])
        .spacing(SPACING_SMALL)
        .align_y(Alignment::Center)
        .push(color_dot(&r.color))
        .push(
            column([])
                .spacing(0)
                .width(Length::Fill)
                .push(text(&r.summary).size(15))
                .push(text(format_result_date(r.start)).size(12)),
        );
    if let Some(loc) = &r.location {
        row = row.push(text(loc).size(12));
    }
    button::custom(row)
        .width(Length::Fill)
        .padding(PADDING_MEDIUM)
        .on_press(Message::SearchSelectResult(
            r.calendar_id.clone(),
            r.uid.clone(),
        ))
        .into()
}

/// Render the search bar (input + results). `results` is the live filter
/// output for the current `query`.
pub fn render_search_bar<'a>(query: &'a str, results: &'a [SearchResult]) -> Element<'a, Message> {
    let placeholder = fl!("search-placeholder");
    let input = text_input(placeholder, query)
        .id(search_input_id())
        .on_input(|s| Message::SearchQueryChanged(s))
        .on_submit(|_| Message::CloseSearch)
        .padding(PADDING_MEDIUM)
        .width(Length::Fill);

    let has_query = !query.trim().is_empty();

    let body: Element<'a, Message> = if !has_query {
        // Idle: just the input.
        container(input).into()
    } else if results.is_empty() {
        column([])
            .spacing(SPACING_SMALL)
            .push(input)
            .push(text(fl!("search-no-results")).size(14))
            .into()
    } else {
        let mut rows = column([]).spacing(0);
        for r in results {
            rows = rows.push(render_result_row(r));
        }
        column([])
            .spacing(SPACING_SMALL)
            .push(input)
            .push(scrollable(rows).height(Length::Shrink).width(Length::Fill))
            .into()
    };

    container(body)
        .width(Length::Fill)
        .padding(SPACING_SMALL)
        .into()
}
