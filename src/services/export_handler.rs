//! Export Handler - Import/Export operations for calendar data.
//!
//! This handler manages importing and exporting calendar data in various formats,
//! primarily iCalendar (.ics) format.

use crate::caldav::{AlertTime, CalendarEvent, RepeatFrequency, TravelTime};
use crate::calendars::CalendarManager;
use chrono::{DateTime, Utc};
use icalendar::{Calendar, Component, DatePerhapsTime, Event, EventLike};
use log::{debug, error, info, warn};
use std::error::Error;
use std::fs;
use std::path::Path;

/// Result type for export operations
#[allow(dead_code)] // Part of export API for future use
pub type ExportResult<T> = Result<T, ExportError>;

/// Error types for export operations
#[allow(dead_code)] // Part of export API for future use
#[derive(Debug)]
pub enum ExportError {
    /// File I/O error
    IoError(String),
    /// Invalid file format
    FormatError(String),
    /// Parse error
    ParseError(String),
    /// Validation error (RFC 5545 compliance)
    ValidationError(String),
    /// Calendar not found
    CalendarNotFound(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::IoError(msg) => write!(f, "I/O error: {}", msg),
            ExportError::FormatError(msg) => write!(f, "Format error: {}", msg),
            ExportError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            ExportError::ValidationError(msg) => write!(f, "Validation error: {}", msg),
            ExportError::CalendarNotFound(id) => write!(f, "Calendar not found: {}", id),
        }
    }
}

impl Error for ExportError {}

/// Export Handler - import/export operations.
#[allow(dead_code)] // Foundation for future import/export feature
pub struct ExportHandler;

impl ExportHandler {
    /// Export a single event to iCalendar format
    #[allow(dead_code)] // Part of export API
    pub fn event_to_ical(event: &CalendarEvent) -> Calendar {
        debug!("ExportHandler: Converting event '{}' (uid={}) to iCal", event.summary, event.uid);

        let mut calendar = Calendar::new();

        let mut ical_event = Event::new();
        ical_event.summary(&event.summary);
        ical_event.uid(&event.uid);
        ical_event.starts(event.start);
        ical_event.ends(event.end);

        if let Some(ref location) = event.location {
            ical_event.location(location);
        }

        if let Some(ref notes) = event.notes {
            ical_event.description(notes);
        }

        if let Some(ref url) = event.url {
            ical_event.url(url);
        }

        calendar.push(ical_event);
        debug!("ExportHandler: Event conversion complete");
        calendar
    }

    /// Export all events from a calendar to iCalendar format
    #[allow(dead_code)] // Part of export API
    pub fn calendar_to_ical(
        manager: &CalendarManager,
        calendar_id: &str,
    ) -> ExportResult<Calendar> {
        info!("ExportHandler: Exporting calendar '{}' to iCal format", calendar_id);

        let calendar = manager
            .sources()
            .iter()
            .find(|c| c.info().id == calendar_id)
            .ok_or_else(|| {
                error!("ExportHandler: Calendar '{}' not found", calendar_id);
                ExportError::CalendarNotFound(calendar_id.to_string())
            })?;

        let events = calendar
            .fetch_events()
            .map_err(|e| {
                error!("ExportHandler: Failed to fetch events: {}", e);
                ExportError::IoError(e.to_string())
            })?;

        debug!("ExportHandler: Found {} events to export", events.len());

        let mut ical = Calendar::new();

        for event in events {
            let mut ical_event = Event::new();
            ical_event.summary(&event.summary);
            ical_event.uid(&event.uid);
            ical_event.starts(event.start);
            ical_event.ends(event.end);

            if let Some(ref location) = event.location {
                ical_event.location(location);
            }

            if let Some(ref notes) = event.notes {
                ical_event.description(notes);
            }

            if let Some(ref url) = event.url {
                ical_event.url(url);
            }

            ical.push(ical_event);
        }

        info!("ExportHandler: Successfully exported calendar '{}'", calendar_id);
        Ok(ical)
    }

    /// Export a calendar to an iCalendar file
    #[allow(dead_code)] // Part of export API
    pub fn export_to_file<P: AsRef<Path>>(
        manager: &CalendarManager,
        calendar_id: &str,
        path: P,
    ) -> ExportResult<()> {
        info!("ExportHandler: Exporting calendar '{}' to file {:?}", calendar_id, path.as_ref());

        let ical = Self::calendar_to_ical(manager, calendar_id)?;
        let ical_string = ical.to_string();

        fs::write(&path, ical_string).map_err(|e| {
            error!("ExportHandler: Failed to write file: {}", e);
            ExportError::IoError(e.to_string())
        })?;

        info!("ExportHandler: Successfully exported to {:?}", path.as_ref());
        Ok(())
    }

    /// Export all calendars to a single iCalendar file
    #[allow(dead_code)] // Part of export API
    pub fn export_all_to_file<P: AsRef<Path>>(
        manager: &CalendarManager,
        path: P,
    ) -> ExportResult<()> {
        info!("ExportHandler: Exporting all calendars to file {:?}", path.as_ref());

        let mut combined = Calendar::new();
        let mut total_events = 0;

        for calendar in manager.sources() {
            if !calendar.is_enabled() {
                debug!("ExportHandler: Skipping disabled calendar '{}'", calendar.info().name);
                continue;
            }

            if let Ok(events) = calendar.fetch_events() {
                debug!("ExportHandler: Adding {} events from '{}'", events.len(), calendar.info().name);
                for event in events {
                    let mut ical_event = Event::new();
                    ical_event.summary(&event.summary);
                    ical_event.uid(&event.uid);
                    ical_event.starts(event.start);
                    ical_event.ends(event.end);

                    if let Some(ref location) = event.location {
                        ical_event.location(location);
                    }

                    if let Some(ref notes) = event.notes {
                        ical_event.description(notes);
                    }

                    combined.push(ical_event);
                    total_events += 1;
                }
            }
        }

        let ical_string = combined.to_string();
        fs::write(&path, ical_string).map_err(|e| {
            error!("ExportHandler: Failed to write file: {}", e);
            ExportError::IoError(e.to_string())
        })?;

        info!("ExportHandler: Exported {} events to {:?}", total_events, path.as_ref());
        Ok(())
    }

    /// Read an iCalendar file (placeholder for future import functionality)
    #[allow(dead_code)] // Part of export API
    pub fn read_ical_file<P: AsRef<Path>>(path: P) -> ExportResult<String> {
        info!("ExportHandler: Reading iCal file {:?}", path.as_ref());
        fs::read_to_string(&path).map_err(|e| {
            error!("ExportHandler: Failed to read file: {}", e);
            ExportError::IoError(e.to_string())
        })
    }

    /// Parse an iCalendar file and return a list of events
    #[allow(dead_code)] // Part of import API
    pub fn parse_ical_file<P: AsRef<Path>>(path: P) -> ExportResult<Vec<CalendarEvent>> {
        info!("ExportHandler: Parsing iCal file {:?}", path.as_ref());
        let ical_string = Self::read_ical_file(&path)?;
        Self::parse_ical_string(&ical_string)
    }

    /// Parse an iCalendar string and return a list of events
    #[allow(dead_code)] // Part of import API
    pub fn parse_ical_string(ical_str: &str) -> ExportResult<Vec<CalendarEvent>> {
        debug!("ExportHandler: Parsing iCal string ({} bytes)", ical_str.len());

        let calendar = ical_str.parse::<Calendar>().map_err(|e| {
            error!("ExportHandler: Failed to parse iCalendar: {}", e);
            ExportError::ParseError(e.to_string())
        })?;

        let mut events = Vec::new();
        for component in calendar.components {
            if let icalendar::CalendarComponent::Event(ical_event) = component {
                match Self::ical_event_to_calendar_event(&ical_event) {
                    Ok(event) => events.push(event),
                    Err(e) => {
                        warn!("ExportHandler: Skipping invalid event: {}", e);
                        continue;
                    }
                }
            }
        }

        info!("ExportHandler: Successfully parsed {} events", events.len());
        Ok(events)
    }

    /// Parse iCalendar string and extract calendar name and events
    /// Returns (calendar_name, events) tuple
    #[allow(dead_code)] // Part of import API
    pub fn parse_ical_string_with_name(ical_str: &str) -> ExportResult<(String, Vec<CalendarEvent>)> {
        debug!("ExportHandler: Parsing iCal string with name ({} bytes)", ical_str.len());

        let calendar = ical_str.parse::<Calendar>().map_err(|e| {
            error!("ExportHandler: Failed to parse iCalendar: {}", e);
            ExportError::ParseError(e.to_string())
        })?;

        // Extract calendar name from X-WR-CALNAME property or use default
        let calendar_name = calendar
            .property_value("X-WR-CALNAME")
            .or_else(|| calendar.property_value("NAME"))
            .unwrap_or("Imported Calendar")
            .to_string();

        debug!("ExportHandler: Extracted calendar name: {}", calendar_name);

        let mut events = Vec::new();
        for component in calendar.components {
            if let icalendar::CalendarComponent::Event(ical_event) = component {
                match Self::ical_event_to_calendar_event(&ical_event) {
                    Ok(event) => events.push(event),
                    Err(e) => {
                        warn!("ExportHandler: Skipping invalid event: {}", e);
                        continue;
                    }
                }
            }
        }

        info!("ExportHandler: Successfully parsed calendar '{}' with {} events", calendar_name, events.len());
        Ok((calendar_name, events))
    }

    /// Convert an icalendar `CalendarDateTime` to a `DateTime<Utc>`.
    ///
    /// The `WithTimezone` variant carries a wall-clock `NaiveDateTime` plus a
    /// `TZID`; the old code treated that wall time as UTC, which shifted every
    /// event by the zone's offset. Here we resolve the `TZID` against
    /// `chrono_tz` and convert to true UTC. If the `TZID` is unknown (custom
    /// VTIMEZONE the crate can't expand) we fall back to treating it as UTC and
    /// log a warning — better than silently misplacing the event.
    fn cal_dt_to_utc(cal_dt: &icalendar::CalendarDateTime) -> Option<DateTime<Utc>> {
        use chrono_tz::Tz;
        use std::str::FromStr;
        match cal_dt {
            icalendar::CalendarDateTime::Floating(dt) => {
                // Floating = "follow the attendee's local zone". We have no zone
                // context here, so treat as UTC (matches the prior behaviour).
                Some(DateTime::from_naive_utc_and_offset(*dt, Utc))
            }
            icalendar::CalendarDateTime::Utc(dt) => Some(*dt),
            icalendar::CalendarDateTime::WithTimezone { date_time, tzid } => {
                use chrono::TimeZone;
                match Tz::from_str(tzid).ok() {
                    Some(tz) => tz
                        .from_local_datetime(date_time)
                        .earliest()
                        .map(|z| z.with_timezone(&Utc))
                        .or_else(|| {
                            warn!(
                                "ExportHandler: unknown TZID '{}' — treating as UTC",
                                tzid
                            );
                            Some(DateTime::from_naive_utc_and_offset(*date_time, Utc))
                        }),
                    None => {
                        warn!(
                            "ExportHandler: unknown TZID '{}' — treating as UTC",
                            tzid
                        );
                        Some(DateTime::from_naive_utc_and_offset(*date_time, Utc))
                    }
                }
            }
        }
    }

    /// Map an RRULE `FREQ` value (plus optional `INTERVAL`) onto the app's
    /// `RepeatFrequency`. Unknown/complex rules become `Custom(raw)` so the
    /// recurrence is preserved rather than dropped.
    fn rrule_to_repeat(rrule: &str) -> RepeatFrequency {
        let upper = rrule.to_ascii_uppercase();
        let freq = upper
            .split(';')
            .find(|part| part.starts_with("FREQ="))
            .and_then(|part| part.strip_prefix("FREQ="))
            .unwrap_or("");
        let interval = upper
            .split(';')
            .find(|part| part.starts_with("INTERVAL="))
            .and_then(|part| part.strip_prefix("INTERVAL="))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);
        match (freq, interval) {
            ("DAILY", 1) => RepeatFrequency::Daily,
            ("WEEKLY", 1) => RepeatFrequency::Weekly,
            ("WEEKLY", 2) => RepeatFrequency::Biweekly,
            ("MONTHLY", 1) => RepeatFrequency::Monthly,
            ("YEARLY", 1) => RepeatFrequency::Yearly,
            ("", _) => RepeatFrequency::Never,
            _ => RepeatFrequency::Custom(rrule.to_string()),
        }
    }

    /// Parse an ISO 8601 duration (e.g. `-PT900S`, `P1D`) into a
    /// `chrono::TimeDelta`.
    ///
    /// icalendar 0.16's `Trigger::try_from` cannot parse negative durations
    /// (its `iso8601::duration` rejects the leading `-`), which means every
    /// before-start alarm — the common case, emitted as
    /// `TRIGGER;RELATED=START:-PT900S` — would be silently dropped on import.
    /// `chrono::TimeDelta` has no ISO 8601 parser of its own, so this is a
    /// minimal hand-rolled one covering the grammar RFC 5545 allows in
    /// `TRIGGER` values.
    fn parse_iso8601_duration(s: &str) -> Option<chrono::TimeDelta> {
        let (sign, rest) = match s.strip_prefix('-') {
            Some(r) => (-1, r),
            None => (1, s),
        };
        let rest = rest.strip_prefix('P')?;
        let (date_part, time_part) = match rest.split_once('T') {
            Some((d, t)) => (d, t),
            None => (rest, ""),
        };
        let mut days: i64 = 0;
        let mut hours: i64 = 0;
        let mut minutes: i64 = 0;
        let mut seconds: f64 = 0.0;
        let mut cur: &str = date_part;
        while !cur.is_empty() {
            let (num, unit) = cur.split_at(
                cur.find(|c: char| !c.is_ascii_digit() && c != '.')
                    .unwrap_or(cur.len()),
            );
            let (unit_char, rest) = match unit.chars().next() {
                Some(c) if c == 'Y' || c == 'M' || c == 'W' || c == 'D' => (c, unit.get(1..).unwrap_or("")),
                _ => return None,
            };
            let n: i64 = num.parse().ok()?;
            match unit_char {
                'D' => days += n,
                'W' => days += n * 7,
                // Years/months are not expressible as a fixed duration; alarm
                // triggers never use them, so reject rather than guess.
                'Y' | 'M' => return None,
                _ => unreachable!(),
            }
            cur = rest;
        }
        cur = time_part;
        while !cur.is_empty() {
            let (num, unit) = cur.split_at(
                cur.find(|c: char| !c.is_ascii_digit() && c != '.')
                    .unwrap_or(cur.len()),
            );
            let (unit_char, rest) = match unit.chars().next() {
                Some(c) if c == 'H' || c == 'M' || c == 'S' => (c, unit.get(1..).unwrap_or("")),
                _ => return None,
            };
            let n: f64 = num.parse().ok()?;
            match unit_char {
                'H' => hours += n as i64,
                'M' => minutes += n as i64,
                'S' => seconds += n,
                _ => unreachable!(),
            }
            cur = rest;
        }
        let total = chrono::TimeDelta::days(days)
            + chrono::TimeDelta::hours(hours)
            + chrono::TimeDelta::minutes(minutes)
            + chrono::TimeDelta::microseconds((seconds * 1_000_000.0) as i64);
        Some(if sign < 0 { -total } else { total })
    }

    /// Parse a VALARM `TRIGGER` property into a `Trigger`.
    fn parse_trigger_prop(prop: &icalendar::Property) -> Option<icalendar::Trigger> {
        use icalendar::Related;
        use std::str::FromStr;
        match prop.params().get("VALUE").map(|p| p.value()) {
            Some("DATE-TIME") => {
                // Date-time triggers are not modelled as alerts.
                None
            }
            _ => {
                let duration = Self::parse_iso8601_duration(prop.value())?;
                let related = prop.get_param_as("RELATED", |s| Related::from_str(s).ok());
                Some(icalendar::Trigger::Duration(duration, related))
            }
        }
    }

    /// Map a VALARM `TRIGGER` duration onto the app's `AlertTime`. Only
    /// before-start durations are modelled; positive (after) triggers and
    /// date-time triggers map to `None`.
    fn trigger_to_alert(trigger: &icalendar::Trigger) -> Option<AlertTime> {
        use icalendar::Trigger;
        match trigger {
            Trigger::Duration(d, related) => {
                let is_start = matches!(related, Some(icalendar::Related::Start));
                let mins = d.num_minutes();
                if mins < 0 && is_start {
                    let mins = -mins;
                    Some(match mins {
                        5 => AlertTime::FiveMinutes,
                        10 => AlertTime::TenMinutes,
                        15 => AlertTime::FifteenMinutes,
                        30 => AlertTime::ThirtyMinutes,
                        60 => AlertTime::OneHour,
                        120 => AlertTime::TwoHours,
                        1440 => AlertTime::OneDay,
                        2880 => AlertTime::TwoDays,
                        10080 => AlertTime::OneWeek,
                        0 => AlertTime::AtTime,
                        n if n > 0 => AlertTime::Custom(n as i32),
                        _ => AlertTime::None,
                    })
                } else {
                    None
                }
            }
            Trigger::DateTime(_) => None,
        }
    }

    /// Convert an icalendar::Event to a CalendarEvent
    #[allow(dead_code)] // Part of import API
    fn ical_event_to_calendar_event(ical_event: &Event) -> ExportResult<CalendarEvent> {
        // Extract UID (required)
        let uid = ical_event
            .get_uid()
            .ok_or_else(|| {
                error!("ExportHandler: Event missing UID");
                ExportError::ParseError("Event missing UID".to_string())
            })?
            .to_string();

        // Extract summary (required)
        let summary = ical_event
            .get_summary()
            .ok_or_else(|| {
                error!("ExportHandler: Event uid={} missing summary", uid);
                ExportError::ParseError(format!("Event uid={} missing summary", uid))
            })?
            .to_string();

        // Extract start time (required)
        let start_prop = ical_event.get_start().ok_or_else(|| {
            error!("ExportHandler: Event uid={} missing start time", uid);
            ExportError::ParseError(format!("Event uid={} missing start time", uid))
        })?;

        let (start, all_day) = match start_prop {
            DatePerhapsTime::DateTime(cal_dt) => (
                Self::cal_dt_to_utc(&cal_dt).ok_or_else(|| {
                    ExportError::ParseError(format!("Event uid={} has unparseable start", uid))
                })?,
                false,
            ),
            DatePerhapsTime::Date(date) => {
                // All-day event - use midnight UTC
                let dt = date
                    .and_hms_opt(0, 0, 0)
                    .ok_or_else(|| ExportError::ParseError("Invalid date".to_string()))?;
                (DateTime::from_naive_utc_and_offset(dt, Utc), true)
            }
        };

        // Extract end time (default to start + 1 hour)
        let end = if let Some(end_prop) = ical_event.get_end() {
            match end_prop {
                DatePerhapsTime::DateTime(cal_dt) => Self::cal_dt_to_utc(&cal_dt).ok_or_else(
                    || ExportError::ParseError(format!("Event uid={} has unparseable end", uid)),
                )?,
                DatePerhapsTime::Date(date) => {
                    let dt = date
                        .and_hms_opt(0, 0, 0)
                        .ok_or_else(|| ExportError::ParseError("Invalid end date".to_string()))?;
                    DateTime::from_naive_utc_and_offset(dt, Utc)
                }
            }
        } else {
            start + chrono::Duration::hours(1)
        };

        // Extract optional fields
        let location = ical_event.get_location().map(|s| s.to_string());
        let notes = ical_event.get_description().map(|s| s.to_string());
        let url = ical_event.get_url().map(|s| s.to_string());

        // Recurrence: RRULE (single) + UNTIL from RRULE.
        let (repeat, repeat_until) = ical_event
            .property_value("RRULE")
            .map(|rrule| {
                let until = rrule
                    .split(';')
                    .find(|part| part.to_ascii_uppercase().starts_with("UNTIL="))
                    .and_then(|part| part.strip_prefix("UNTIL="))
                    .and_then(|s| s.strip_suffix("Z"))
                    .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y%m%d").ok());
                (Self::rrule_to_repeat(rrule), until)
            })
            .unwrap_or((RepeatFrequency::Never, None));

        // Exception dates: EXDATE (multi-valued). All-day events carry bare
        // dates (YYYYMMDD); timed events carry full date-times — we only model
        // the date part either way, which is what the app's exception list uses.
        let exception_dates = ical_event
            .multi_properties()
            .get("EXDATE")
            .map(|props| {
                props
                    .iter()
                    .filter_map(|p| {
                        let v = p.value();
                        // Take the leading 8 chars (YYYYMMDD) of the value.
                        let date_part = &v[..v.len().min(8)];
                        chrono::NaiveDate::parse_from_str(date_part, "%Y%m%d").ok()
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Reminders: first VALARM child with a parseable before-start trigger.
        let alert = ical_event
            .components()
            .iter()
            .filter(|c| c.component_kind().eq_ignore_ascii_case("VALARM"))
            .find_map(|c| {
                c.properties().get("TRIGGER").and_then(|p| {
                    Self::parse_trigger_prop(p).and_then(|t| Self::trigger_to_alert(&t))
                })
            })
            .unwrap_or(AlertTime::None);

        debug!("ExportHandler: Parsed event uid={}", uid);

        Ok(CalendarEvent {
            uid,
            summary,
            location,
            all_day,
            start,
            end,
            travel_time: TravelTime::None,
            repeat,
            repeat_until,
            exception_dates,
            invitees: vec![],
            alert,
            alert_second: None,
            attachments: vec![],
            url,
            notes,
        })
    }

    /// Import events from a file into a specific calendar
    /// Returns the number of events imported (skips duplicates based on UID)
    #[allow(dead_code)] // Part of import API
    pub fn import_from_file<P: AsRef<Path>>(
        manager: &mut CalendarManager,
        calendar_id: &str,
        path: P,
    ) -> ExportResult<usize> {
        info!("ExportHandler: Importing events from {:?} into calendar '{}'", path.as_ref(), calendar_id);

        // Parse the file
        let events = Self::parse_ical_file(&path)?;

        // Get the target calendar
        let calendar = manager
            .sources_mut()
            .iter_mut()
            .find(|c| c.info().id == calendar_id)
            .ok_or_else(|| {
                error!("ExportHandler: Calendar '{}' not found", calendar_id);
                ExportError::CalendarNotFound(calendar_id.to_string())
            })?;

        // Get existing event UIDs to detect duplicates
        let existing_events = calendar.fetch_events().map_err(|e| {
            error!("ExportHandler: Failed to fetch existing events: {}", e);
            ExportError::IoError(e.to_string())
        })?;
        let existing_uids: std::collections::HashSet<_> =
            existing_events.iter().map(|e| e.uid.as_str()).collect();

        // Import events, skipping duplicates
        let mut imported_count = 0;
        let total_events = events.len();
        for event in events {
            if existing_uids.contains(event.uid.as_str()) {
                debug!("ExportHandler: Skipping duplicate event uid={}", event.uid);
                continue;
            }

            calendar.add_event(event).map_err(|e| {
                error!("ExportHandler: Failed to add event: {}", e);
                ExportError::IoError(e.to_string())
            })?;
            imported_count += 1;
        }

        info!("ExportHandler: Successfully imported {} events (skipped {} duplicates)",
              imported_count, total_events - imported_count);
        Ok(imported_count)
    }

    /// Validate an iCalendar file for RFC 5545 compliance
    /// Returns Ok(()) if valid, Err with validation errors otherwise
    pub fn validate_ical_file<P: AsRef<Path>>(path: P) -> ExportResult<()> {
        info!("ExportHandler: Validating iCal file {:?}", path.as_ref());

        let ical_string = Self::read_ical_file(&path)?;
        Self::validate_ical_string(&ical_string)
    }

    /// Validate an iCalendar string for RFC 5545 compliance
    pub fn validate_ical_string(ical_str: &str) -> ExportResult<()> {
        // Check minimum length
        if ical_str.len() < 50 {
            return Err(ExportError::ValidationError(
                "File too short to be valid iCalendar".to_string()
            ));
        }

        // Check for required VCALENDAR wrapper
        if !ical_str.contains("BEGIN:VCALENDAR") || !ical_str.contains("END:VCALENDAR") {
            return Err(ExportError::ValidationError(
                "Missing required VCALENDAR wrapper (RFC 5545 §3.4)".to_string()
            ));
        }

        // Check for VERSION property (required by RFC 5545 §3.7.4)
        if !ical_str.contains("VERSION:") {
            return Err(ExportError::ValidationError(
                "Missing required VERSION property (RFC 5545 §3.7.4)".to_string()
            ));
        }

        // Check for PRODID property (required by RFC 5545 §3.7.3)
        if !ical_str.contains("PRODID:") {
            warn!("ExportHandler: Missing PRODID property (RFC 5545 §3.7.3) - continuing anyway");
        }

        // Try to parse to verify structure
        let calendar = ical_str.parse::<Calendar>().map_err(|e| {
            error!("ExportHandler: iCalendar structure validation failed: {}", e);
            ExportError::ValidationError(format!("Invalid iCalendar structure: {}", e))
        })?;

        // Validate each event component
        let mut event_count = 0;
        for component in &calendar.components {
            if let icalendar::CalendarComponent::Event(event) = component {
                Self::validate_event_component(event)?;
                event_count += 1;
            }
        }

        if event_count == 0 {
            warn!("ExportHandler: No VEVENT components found in calendar");
        }

        info!("ExportHandler: Validation successful - {} events", event_count);
        Ok(())
    }

    /// Validate a single event component for RFC 5545 compliance
    fn validate_event_component(event: &Event) -> ExportResult<()> {
        // UID is required by RFC 5545 §3.8.4.7
        let uid = event.get_uid().ok_or_else(|| {
            ExportError::ValidationError("Event missing required UID property (RFC 5545 §3.8.4.7)".to_string())
        })?;

        // DTSTAMP is required by RFC 5545 §3.8.7.2
        if event.get_timestamp().is_none() {
            warn!("ExportHandler: Event uid={} missing DTSTAMP (RFC 5545 §3.8.7.2) - continuing anyway", uid);
        }

        // DTSTART is required for most events (RFC 5545 §3.8.2.4)
        let start = event.get_start().ok_or_else(|| {
            ExportError::ValidationError(format!(
                "Event uid={} missing required DTSTART property (RFC 5545 §3.8.2.4)", uid
            ))
        })?;

        // If DTEND exists, validate it's after DTSTART
        if let Some(end) = event.get_end() {
            // Compare start and end times
            let start_is_before_end = match (start, end) {
                (DatePerhapsTime::DateTime(start_dt), DatePerhapsTime::DateTime(end_dt)) => {
                    // For timed events, ensure end > start
                    match (start_dt, end_dt) {
                        (icalendar::CalendarDateTime::Utc(s), icalendar::CalendarDateTime::Utc(e)) => s < e,
                        (icalendar::CalendarDateTime::Floating(s), icalendar::CalendarDateTime::Floating(e)) => s < e,
                        _ => true, // Different timezone types, hard to compare - allow
                    }
                },
                (DatePerhapsTime::Date(start_date), DatePerhapsTime::Date(end_date)) => {
                    // For all-day events, end should be after or equal to start
                    start_date <= end_date
                },
                _ => true, // Mixed date/datetime - allow
            };

            if !start_is_before_end {
                return Err(ExportError::ValidationError(format!(
                    "Event uid={} has DTEND before DTSTART", uid
                )));
            }
        }

        Ok(())
    }

    /// Detect iCalendar dialect/producer from PRODID
    /// Returns detected dialect for handling quirks
    #[allow(dead_code)] // Future use for dialect-specific handling
    pub fn detect_dialect(ical_str: &str) -> Option<&'static str> {
        // Extract PRODID line
        for line in ical_str.lines() {
            if line.starts_with("PRODID:") {
                let prodid = line.trim_start_matches("PRODID:").trim();

                // Detect common producers
                if prodid.contains("Google") {
                    return Some("google");
                } else if prodid.contains("Microsoft") || prodid.contains("Outlook") {
                    return Some("outlook");
                } else if prodid.contains("Apple") || prodid.contains("iCal") || prodid.contains("macOS") {
                    return Some("apple");
                } else if prodid.contains("Mozilla") || prodid.contains("Thunderbird") {
                    return Some("thunderbird");
                } else if prodid.contains("Yahoo") {
                    return Some("yahoo");
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caldav::{AlertTime, RepeatFrequency, TravelTime};
    use chrono::{TimeZone, Utc};

    fn create_test_event() -> CalendarEvent {
        CalendarEvent {
            uid: "test-export-1".to_string(),
            summary: "Test Export Event".to_string(),
            location: Some("Test Location".to_string()),
            all_day: false,
            start: Utc.with_ymd_and_hms(2025, 12, 1, 10, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2025, 12, 1, 11, 0, 0).unwrap(),
            travel_time: TravelTime::None,
            repeat: RepeatFrequency::Never,
            repeat_until: None,
            exception_dates: vec![],
            invitees: vec![],
            alert: AlertTime::None,
            alert_second: None,
            attachments: vec![],
            url: None,
            notes: Some("Test notes".to_string()),
        }
    }

    #[test]
    fn test_event_to_ical() {
        let event = create_test_event();
        let ical = ExportHandler::event_to_ical(&event);
        let ical_string = ical.to_string();

        assert!(ical_string.contains("BEGIN:VCALENDAR"));
        assert!(ical_string.contains("BEGIN:VEVENT"));
        assert!(ical_string.contains("Test Export Event"));
        assert!(ical_string.contains("END:VEVENT"));
        assert!(ical_string.contains("END:VCALENDAR"));
    }

    #[test]
    fn test_rrule_to_repeat() {
        assert_eq!(ExportHandler::rrule_to_repeat("FREQ=DAILY"), RepeatFrequency::Daily);
        assert_eq!(ExportHandler::rrule_to_repeat("FREQ=WEEKLY"), RepeatFrequency::Weekly);
        assert_eq!(
            ExportHandler::rrule_to_repeat("FREQ=WEEKLY;INTERVAL=2"),
            RepeatFrequency::Biweekly
        );
        assert_eq!(
            ExportHandler::rrule_to_repeat("FREQ=MONTHLY"),
            RepeatFrequency::Monthly
        );
        assert_eq!(ExportHandler::rrule_to_repeat("FREQ=YEARLY"), RepeatFrequency::Yearly);
        // Known limitation: mapping is by FREQ+INTERVAL only — BYDAY is
        // ignored, so a multi-day weekly rule degrades to plain Weekly.
        assert_eq!(
            ExportHandler::rrule_to_repeat("FREQ=WEEKLY;BYDAY=MO,WE,FR"),
            RepeatFrequency::Weekly
        );
        // UNTIL is not a FREQ part — the rule still maps by FREQ/INTERVAL.
        assert_eq!(
            ExportHandler::rrule_to_repeat("FREQ=WEEKLY;INTERVAL=2;UNTIL=20261231T000000Z"),
            RepeatFrequency::Biweekly
        );
    }

    /// The hand-rolled ISO 8601 duration parser must handle the sign, which
    /// icalendar 0.16's own parser rejects.
    #[test]
    fn test_parse_iso8601_duration() {
        use chrono::TimeDelta;
        assert_eq!(
            ExportHandler::parse_iso8601_duration("-PT900S"),
            Some(TimeDelta::seconds(-900))
        );
        assert_eq!(
            ExportHandler::parse_iso8601_duration("PT15M"),
            Some(TimeDelta::minutes(15))
        );
        assert_eq!(
            ExportHandler::parse_iso8601_duration("-P1DT2H"),
            Some(TimeDelta::days(-1) + TimeDelta::hours(-2))
        );
        assert_eq!(
            ExportHandler::parse_iso8601_duration("-PT30.5S"),
            Some(TimeDelta::milliseconds(-30_500))
        );
        // Positive durations parse too (after-start alarms).
        assert_eq!(
            ExportHandler::parse_iso8601_duration("PT900S"),
            Some(TimeDelta::seconds(900))
        );
        // Missing P prefix, Y/M units, or garbage are not valid alarm durations.
        assert_eq!(ExportHandler::parse_iso8601_duration("T900S"), None);
        assert_eq!(ExportHandler::parse_iso8601_duration("-P1M"), None);
        assert_eq!(ExportHandler::parse_iso8601_duration("garbage"), None);
    }

    /// Regression: a `DTSTART;TZID=...` wall clock must be converted to true
    /// UTC, not treated as UTC. New York on 2025-01-01 is EST (UTC-5), so
    /// 09:00 local is 14:00Z.
    #[test]
    fn test_tzid_start_converted_to_utc() {
        let ics = "BEGIN:VCALENDAR\r\n\
                   VERSION:2.0\r\n\
                   BEGIN:VEVENT\r\n\
                   UID:tz-test-1\r\n\
                   SUMMARY:Timezone Test\r\n\
                   DTSTART;TZID=America/New_York:20250101T090000\r\n\
                   DTEND;TZID=America/New_York:20250101T100000\r\n\
                   END:VEVENT\r\n\
                   END:VCALENDAR";
        let events = ExportHandler::parse_ical_string(ics).expect("parse");
        assert_eq!(events.len(), 1);
        assert_eq!(
            events[0].start,
            Utc.with_ymd_and_hms(2025, 1, 1, 14, 0, 0).unwrap()
        );
        assert_eq!(
            events[0].end,
            Utc.with_ymd_and_hms(2025, 1, 1, 15, 0, 0).unwrap()
        );
    }

    /// A DST-affected date: New York on 2025-06-15 is EDT (UTC-4), so
    /// 09:00 local is 13:00Z — the offset must come from the zone, not be
    /// hardcoded.
    #[test]
    fn test_tzid_start_dst_offset() {
        let ics = "BEGIN:VCALENDAR\r\n\
                   VERSION:2.0\r\n\
                   BEGIN:VEVENT\r\n\
                   UID:tz-test-2\r\n\
                   SUMMARY:Timezone Test DST\r\n\
                   DTSTART;TZID=America/New_York:20250615T090000\r\n\
                   DTEND;TZID=America/New_York:20250615T100000\r\n\
                   END:VEVENT\r\n\
                   END:VCALENDAR";
        let events = ExportHandler::parse_ical_string(ics).expect("parse");
        assert_eq!(
            events[0].start,
            Utc.with_ymd_and_hms(2025, 6, 15, 13, 0, 0).unwrap()
        );
    }
}
