//! Calendar sources: the `CalendarSource` trait plus the local (SQLite)
//! and CalDAV implementations, and on-disk configuration.

mod caldav_calendar;
mod calendar_source;
mod config;
mod local_calendar;

pub use caldav_calendar::CalDavCalendar;
pub use calendar_source::{CalendarSource, CalendarType};
pub use config::{CalendarConfig, CalendarManagerConfig};
pub use local_calendar::LocalCalendar;

use crate::caldav::{CalendarEvent, RepeatFrequency};
use crate::components::DisplayEvent;
use crate::database::Database;
use chrono::{Datelike, Duration, Months, NaiveDate, Timelike};
use log::{debug, info};
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex};

/// Manager for all calendar sources
#[derive(Debug)]
pub struct CalendarManager {
    sources: Vec<Box<dyn CalendarSource>>,
    /// Shared database connection
    db: Arc<Mutex<Database>>,
}

impl CalendarManager {
    /// Create a new CalendarManager with a database connection
    pub fn new() -> Self {
        info!("CalendarManager: Initializing");

        // Open or create the database
        let db = Database::open().expect("Failed to open database");
        let db = Arc::new(Mutex::new(db));

        CalendarManager {
            sources: Vec::new(),
            db,
        }
    }

    /// Create a new CalendarManager, loading calendars from config
    /// If no calendars exist, creates default ones
    pub fn with_defaults() -> Self {
        info!("CalendarManager: Loading with defaults");
        let mut manager = Self::new();
        let db = manager.db.clone();

        // Try to load calendars from config
        let config = CalendarManagerConfig::load().unwrap_or_default();

        if config.calendars.is_empty() {
            info!("CalendarManager: No saved calendars, creating defaults");
            // No saved calendars, create defaults
            manager.add_source(Box::new(LocalCalendar::with_color(
                "personal".to_string(),
                "Personal".to_string(),
                "#3B82F6".to_string(),
                db.clone(),
            )));

            manager.add_source(Box::new(LocalCalendar::with_color(
                "work".to_string(),
                "Work".to_string(),
                "#8B5CF6".to_string(),
                db,
            )));

            // Save the defaults
            manager.save_config().ok();
        } else {
            info!(
                "CalendarManager: Loading {} calendars from config",
                config.calendars.len()
            );
            // Load calendars from config
            for cal_config in &config.calendars {
                debug!(
                    "CalendarManager: Loading calendar '{}' ({})",
                    cal_config.name, cal_config.id
                );
                // Case-insensitive: configs written before the
                // `as_config()` fix may hold the lowercase `"caldav"`
                // form, current ones hold `"CalDav"`.
                if cal_config
                    .calendar_type
                    .eq_ignore_ascii_case(CalendarType::CalDav.as_config())
                {
                    // CalDAV: password lives only in the keyring. A missing
                    // credential or construct failure logs (host only) and
                    // skips the calendar rather than failing startup.
                    match Self::load_caldav_source(cal_config) {
                        Ok(mut calendar) => {
                            calendar.info_mut().color = cal_config.color.clone();
                            calendar.info_mut().enabled = cal_config.enabled;
                            manager.add_source(Box::new(calendar));
                        }
                        Err(e) => {
                            log::warn!(
                                "CalendarManager: Skipping CalDAV calendar '{}' ({}): {}",
                                cal_config.name,
                                cal_config.id,
                                e
                            );
                        }
                    }
                    continue;
                }
                let mut calendar =
                    LocalCalendar::new(cal_config.id.clone(), cal_config.name.clone(), db.clone());
                // Apply saved settings
                calendar.info_mut().color = cal_config.color.clone();
                calendar.info_mut().enabled = cal_config.enabled;
                manager.add_source(Box::new(calendar));
            }
        }

        info!(
            "CalendarManager: Initialized with {} calendars",
            manager.sources.len()
        );
        manager
    }

    /// Construct a CalDAV calendar source from its saved config.
    ///
    /// The password is loaded from the keyring (never stored in the config
    /// file). Any failure — missing config fields, keyring miss, client
    /// construction error — is returned to the caller, which logs and skips
    /// the calendar rather than failing startup.
    fn load_caldav_source(cal_config: &CalendarConfig) -> Result<CalDavCalendar, Box<dyn Error>> {
        use crate::services::CalDavCredentials;

        let server_url = cal_config
            .server_url
            .as_ref()
            .ok_or_else(|| "missing server_url".to_string())?;
        let username = cal_config
            .username
            .as_ref()
            .ok_or_else(|| "missing username".to_string())?;

        let password = CalDavCredentials::load(server_url, username)?;

        CalDavCalendar::new(
            cal_config.id.clone(),
            cal_config.name.clone(),
            server_url.clone(),
            username.clone(),
            password,
        )
        .map_err(|e| e.into())
    }

    /// Add a new local calendar
    pub fn add_local_calendar(&mut self, id: String, name: String, color: String) {
        let calendar = LocalCalendar::with_color(id, name, color, self.db.clone());
        self.add_source(Box::new(calendar));
        self.save_config().ok();
    }

    /// Remove a calendar by ID and delete all its events
    pub fn delete_calendar(&mut self, id: &str) -> bool {
        // First delete all events for this calendar from database
        if let Ok(db) = self.db.lock() {
            match db.delete_events_for_calendar(id) {
                Ok(count) => {
                    log::info!("Deleted {} events for calendar '{}'", count, id);
                }
                Err(e) => {
                    log::error!("Failed to delete events for calendar '{}': {}", id, e);
                    // Continue anyway to remove calendar from sources
                }
            }
        }

        // Remove from sources
        if let Some(index) = self.sources.iter().position(|s| s.info().id == id) {
            self.sources.remove(index);

            // Update config file
            if let Ok(mut config) = CalendarManagerConfig::load() {
                config.remove_calendar(id);
                config.save().ok();
            }

            return true;
        }
        false
    }

    /// Get the shared database connection
    #[allow(dead_code)] // Reserved for future database operations
    pub fn database(&self) -> Arc<Mutex<Database>> {
        self.db.clone()
    }

    /// Add a calendar source to the manager
    pub fn add_source(&mut self, source: Box<dyn CalendarSource>) {
        self.sources.push(source);
    }

    /// Remove a calendar source by ID
    #[allow(dead_code)] // Reserved for future calendar removal
    pub fn remove_source(&mut self, id: &str) -> bool {
        if let Some(index) = self.sources.iter().position(|s| s.info().id == id) {
            self.sources.remove(index);
            true
        } else {
            false
        }
    }

    /// Get all calendar sources
    pub fn sources(&self) -> &[Box<dyn CalendarSource>] {
        &self.sources
    }

    /// Get a mutable reference to all sources
    pub fn sources_mut(&mut self) -> &mut [Box<dyn CalendarSource>] {
        &mut self.sources
    }

    /// Get all events from all enabled calendars
    #[allow(dead_code)] // Reserved for future event filtering
    pub fn get_all_events(&self) -> Vec<CalendarEvent> {
        let mut all_events = Vec::new();
        for source in &self.sources {
            if source.is_enabled() {
                if let Ok(events) = source.fetch_events() {
                    all_events.extend(events);
                }
            }
        }
        all_events
    }

    /// Get events for a specific date from all enabled calendars
    #[allow(dead_code)] // Reserved for future day view filtering
    pub fn get_events_for_date(&self, date: chrono::NaiveDate) -> Vec<CalendarEvent> {
        self.get_all_events()
            .into_iter()
            .filter(|e| e.start.date_naive() == date)
            .collect()
    }

    /// Get events for a specific month from all enabled calendars
    #[allow(dead_code)] // Reserved for future month filtering
    pub fn get_events_for_month(&self, year: i32, month: u32) -> Vec<CalendarEvent> {
        self.get_all_events()
            .into_iter()
            .filter(|e| {
                let event_date = e.start.date_naive();
                event_date.year() == year && event_date.month() == month
            })
            .collect()
    }

    /// Expand a recurring event into multiple occurrences within a date range
    /// Returns a vector of (occurrence_date, event) tuples
    /// Skips exception dates (dates where the recurring event was deleted for a single occurrence)
    pub fn expand_recurring_event(
        event: &CalendarEvent,
        range_start: NaiveDate,
        range_end: NaiveDate,
    ) -> Vec<(NaiveDate, CalendarEvent)> {
        // Non-recurring events return a single occurrence
        if matches!(event.repeat, RepeatFrequency::Never) {
            let event_date = event.start.date_naive();
            if event_date >= range_start && event_date <= range_end {
                return vec![(event_date, event.clone())];
            } else {
                return vec![];
            }
        }

        let mut occurrences = Vec::new();
        let event_start_date = event.start.date_naive();

        // Determine the end date for recurrence
        let recurrence_end = event.repeat_until.unwrap_or(range_end);

        // Start from the event's start date, fast-forwarding to the first
        // occurrence at or after range_start for the simple frequencies.
        // Without this, a daily event that started >1000 days before the
        // range would hit the iteration cap and return nothing.
        //
        // For month/year steps we jump to the k-th occurrence with a single
        // `checked_add_months` and then step forward at most `interval`
        // times: each step is defined relative to the previous date, and
        // `checked_add_months` is not compositional (Jan 31 + 2 months =
        // Mar 31, but two single-month steps land on Mar 28), so the jump
        // must be one addition from the start date, followed by normal steps.
        let mut current_date = event_start_date;
        if range_start > current_date {
            current_date = match &event.repeat {
                RepeatFrequency::Daily => range_start,
                RepeatFrequency::Weekly => {
                    let weeks = ((range_start - event_start_date).num_days() + 6) / 7;
                    event_start_date + Duration::weeks(weeks)
                }
                RepeatFrequency::Biweekly => {
                    let weeks = ((range_start - event_start_date).num_days() + 13) / 14;
                    event_start_date + Duration::weeks(weeks * 2)
                }
                RepeatFrequency::Monthly | RepeatFrequency::Yearly => {
                    let step_months = match event.repeat {
                        RepeatFrequency::Monthly => 1,
                        _ => 12,
                    };
                    let start_months = i64::from(event_start_date.year()) * 12
                        + i64::from(event_start_date.month());
                    let range_months =
                        i64::from(range_start.year()) * 12 + i64::from(range_start.month());
                    let k = ((range_months - start_months).max(0) / step_months) * step_months;
                    let mut d = event_start_date
                        .checked_add_months(Months::new(k as u32))
                        .unwrap_or(event_start_date);
                    while d < range_start {
                        d = d
                            .checked_add_months(Months::new(step_months as u32))
                            .unwrap_or(d + Duration::days(30 * step_months));
                    }
                    d
                }
                // Custom rules have no closed-form step; iterate as before.
                _ => current_date,
            };
        }

        // Limit iterations to prevent infinite loops (max 1000 occurrences per query)
        let max_iterations = 1000;
        let mut iteration_count = 0;

        while current_date <= recurrence_end
            && current_date <= range_end
            && iteration_count < max_iterations
        {
            iteration_count += 1;

            // Only add if within the visible range AND not an exception date
            if current_date >= range_start && !event.exception_dates.contains(&current_date) {
                // Create a clone of the event with adjusted dates
                let duration = event.end - event.start;
                let mut occurrence = event.clone();
                occurrence.start = current_date.and_time(event.start.time()).and_utc();
                occurrence.end = occurrence.start + duration;

                // Generate unique UID for each occurrence by appending the date
                // This ensures deduplication logic in views doesn't skip occurrences
                occurrence.uid = format!("{}_{}", event.uid, current_date.format("%Y%m%d"));

                occurrences.push((current_date, occurrence));
            }

            // Advance to next occurrence based on repeat frequency
            current_date = match &event.repeat {
                RepeatFrequency::Daily => current_date + Duration::days(1),
                RepeatFrequency::Weekly => current_date + Duration::weeks(1),
                RepeatFrequency::Biweekly => current_date + Duration::weeks(2),
                RepeatFrequency::Monthly => {
                    // Add one month, handling month boundaries
                    current_date
                        .checked_add_months(Months::new(1))
                        .unwrap_or(current_date + Duration::days(30))
                }
                RepeatFrequency::Yearly => {
                    // Add one year
                    current_date
                        .checked_add_months(Months::new(12))
                        .unwrap_or(current_date + Duration::days(365))
                }
                RepeatFrequency::Custom(rrule) => {
                    match Self::next_custom_occurrence(current_date, event_start_date, rrule) {
                        Some(next) => next,
                        None => break,
                    }
                }
                RepeatFrequency::Never => break,
            };
        }

        occurrences
    }

    /// Compute the next occurrence date for a `Custom` RRULE string, or `None`
    /// if the rule is unparseable (which stops the expansion loop).
    /// `start_date` is the event's DTSTART date — the anchor that `INTERVAL`
    /// is relative to, per RFC 5545. Supports:
    /// - `FREQ=DAILY|WEEKLY|MONTHLY|YEARLY` with optional `INTERVAL`
    /// - `FREQ=WEEKLY;BYDAY=MO,TU,...` — listed weekdays in order, only in
    ///   weeks that are a multiple of `interval` weeks after the start week
    /// - `FREQ=MONTHLY;BYMONTHDAY=n[,n...]` — listed day(s) of month, in
    ///   months a multiple of `interval` months after the start month
    /// - `FREQ=YEARLY;BYMONTH=m;BYMONTHDAY=n` — n-th of month m, in years a
    ///   multiple of `interval` years after the start year
    /// Rules without a `FREQ` component stop expansion rather than guessing.
    fn next_custom_occurrence(
        current_date: NaiveDate,
        start_date: NaiveDate,
        rrule: &str,
    ) -> Option<NaiveDate> {
        let upper = rrule.to_ascii_uppercase();
        let parts: Vec<&str> = upper.split(';').collect();
        let freq = parts
            .iter()
            .find(|p| p.starts_with("FREQ="))
            .and_then(|p| p.strip_prefix("FREQ="))?;
        let interval = parts
            .iter()
            .find(|p| p.starts_with("INTERVAL="))
            .and_then(|p| p.strip_prefix("INTERVAL="))
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1)
            .max(1) as i64;
        let byday = parts
            .iter()
            .find(|p| p.starts_with("BYDAY="))
            .and_then(|p| p.strip_prefix("BYDAY="));
        let bymonthday = parts
            .iter()
            .find(|p| p.starts_with("BYMONTHDAY="))
            .and_then(|p| p.strip_prefix("BYMONTHDAY="));
        let bymonth = parts
            .iter()
            .find(|p| p.starts_with("BYMONTH="))
            .and_then(|p| p.strip_prefix("BYMONTH="));

        match freq {
            "DAILY" => Some(current_date + Duration::days(interval)),
            "WEEKLY" => match byday {
                Some(days) if !days.is_empty() => {
                    let weekdays: Vec<chrono::Weekday> = days
                        .split(',')
                        .filter_map(|d| match d.trim() {
                            "MO" => Some(chrono::Weekday::Mon),
                            "TU" => Some(chrono::Weekday::Tue),
                            "WE" => Some(chrono::Weekday::Wed),
                            "TH" => Some(chrono::Weekday::Thu),
                            "FR" => Some(chrono::Weekday::Fri),
                            "SA" => Some(chrono::Weekday::Sat),
                            "SU" => Some(chrono::Weekday::Sun),
                            _ => None,
                        })
                        .collect();
                    if weekdays.is_empty() {
                        None
                    } else {
                        // Week index of a date = weeks between its
                        // week-starting Monday and the start date's Monday.
                        // (A plain day-difference from DTSTART is wrong when
                        // DTSTART is not a Monday.)
                        let start_monday = start_date
                            - Duration::days(start_date.weekday().num_days_from_monday() as i64);
                        // Advance day by day to the next listed weekday in an
                        // in-cycle week. Bounded: one full cycle plus a week.
                        let mut d = current_date;
                        for _ in 0..7 * interval + 7 {
                            d = d + Duration::days(1);
                            let d_monday =
                                d - Duration::days(d.weekday().num_days_from_monday() as i64);
                            let week_idx = (d_monday - start_monday).num_days() / 7;
                            if week_idx % interval == 0 && weekdays.contains(&d.weekday()) {
                                return Some(d);
                            }
                        }
                        None
                    }
                }
                _ => Some(current_date + Duration::weeks(interval)),
            },
            "MONTHLY" => match bymonthday {
                Some(days) => {
                    let days: Vec<i64> = days
                        .split(',')
                        .filter_map(|d| d.trim().parse::<i64>().ok())
                        .collect();
                    if days.is_empty() {
                        None
                    } else {
                        let start_idx =
                            i64::from(start_date.year()) * 12 + i64::from(start_date.month());
                        let mut idx =
                            i64::from(current_date.year()) * 12 + i64::from(current_date.month());
                        // Bounded: one full interval cycle plus a year.
                        for _ in 0..interval + 12 {
                            if (idx - start_idx) % interval == 0 {
                                let y = ((idx - 1) / 12) as i32;
                                let m = ((idx - 1) % 12 + 1) as u32;
                                for &dy in &days {
                                    let dim = NaiveDate::from_ymd_opt(y, m, 1)
                                        .map(|d| d.num_days_in_month() as i64)
                                        .unwrap_or(31);
                                    if dy > dim {
                                        continue;
                                    }
                                    if let Some(c) = NaiveDate::from_ymd_opt(y, m, dy as u32) {
                                        if c > current_date {
                                            return Some(c);
                                        }
                                    }
                                }
                            }
                            idx += 1;
                        }
                        None
                    }
                }
                _ => current_date
                    .checked_add_months(Months::new(interval as u32))
                    .or_else(|| Some(current_date + Duration::days(30 * interval))),
            },
            "YEARLY" => {
                let month = bymonth
                    .and_then(|m| m.split(',').next())
                    .and_then(|m| m.trim().parse::<i64>().ok());
                let day = bymonthday
                    .and_then(|d| d.split(',').next())
                    .and_then(|d| d.trim().parse::<i64>().ok());
                match (month, day) {
                    (Some(m), Some(dy)) if (1..=12).contains(&m) && (1..=31).contains(&dy) => {
                        let start_year = start_date.year();
                        let mut y = current_date.year();
                        // Bounded: one full interval cycle plus two years.
                        for _ in 0..interval as i32 + 2 {
                            if (y - start_year) % interval as i32 == 0 {
                                let dim = NaiveDate::from_ymd_opt(y, m as u32, 1)
                                    .map(|d| d.num_days_in_month() as i64)
                                    .unwrap_or(31);
                                if dy <= dim {
                                    if let Some(c) = NaiveDate::from_ymd_opt(y, m as u32, dy as u32)
                                    {
                                        if c > current_date {
                                            return Some(c);
                                        }
                                    }
                                }
                            }
                            y += 1;
                        }
                        None
                    }
                    _ => current_date
                        .checked_add_months(Months::new(12 * interval as u32))
                        .or_else(|| Some(current_date + Duration::days(365 * interval))),
                }
            }
            _ => None,
        }
    }

    /// Get events for a specific month grouped by date, with calendar colors.
    /// Includes events from adjacent months that would be visible in the month view.
    /// Returns a HashMap where key is NaiveDate and value is Vec of DisplayEvents.
    pub fn get_display_events_for_month(
        &self,
        year: i32,
        month: u32,
    ) -> HashMap<chrono::NaiveDate, Vec<DisplayEvent>> {
        use chrono::NaiveDate;

        let mut events_by_date: HashMap<NaiveDate, Vec<DisplayEvent>> = HashMap::new();

        // Calculate date range for the month view (includes adjacent month days visible in the grid)
        // The grid can show up to 6 days from prev month and up to 13 days from next month
        let first_of_month = NaiveDate::from_ymd_opt(year, month, 1).unwrap();

        // Start from up to 6 days before (max days from prev month in grid)
        let range_start = first_of_month - chrono::Duration::days(6);
        // End up to 13 days after the month ends (max days from next month in grid)
        let days_in_month = if month == 12 {
            NaiveDate::from_ymd_opt(year + 1, 1, 1)
                .unwrap()
                .signed_duration_since(first_of_month)
                .num_days()
        } else {
            NaiveDate::from_ymd_opt(year, month + 1, 1)
                .unwrap()
                .signed_duration_since(first_of_month)
                .num_days()
        };
        let range_end = first_of_month + chrono::Duration::days(days_in_month + 13);

        for source in &self.sources {
            if !source.is_enabled() {
                continue;
            }

            let calendar_color = source.info().color.clone();

            if let Ok(events) = source.fetch_events() {
                for event in events {
                    // Expand recurring events into individual occurrences
                    let occurrences = Self::expand_recurring_event(&event, range_start, range_end);

                    for (_occurrence_date, occurrence_event) in occurrences {
                        let event_start = occurrence_event.start.date_naive();
                        let event_end = occurrence_event.end.date_naive();

                        // For all-day events, add to each day in the range
                        // For multi-day events (end > start), show on each day
                        if occurrence_event.all_day && event_end > event_start {
                            // Multi-day event: iterate through each day
                            let mut current = event_start;
                            while current <= event_end && current <= range_end {
                                if current >= range_start {
                                    let display_event = DisplayEvent {
                                        calendar_id: source.info().id.clone(),
                                        uid: occurrence_event.uid.clone(),
                                        summary: occurrence_event.summary.clone(),
                                        color: calendar_color.clone(),
                                        all_day: true,
                                        start_time: None,
                                        end_time: None,
                                        span_start: Some(event_start),
                                        span_end: Some(event_end),
                                    };
                                    events_by_date
                                        .entry(current)
                                        .or_default()
                                        .push(display_event);
                                }
                                current = current.succ_opt().unwrap_or(current);
                            }
                        } else {
                            // Single-day event: only add to start date
                            if event_start >= range_start && event_start <= range_end {
                                // Extract start and end time for timed events
                                let (start_time, end_time) = if occurrence_event.all_day {
                                    (None, None)
                                } else {
                                    (
                                        Some(
                                            chrono::NaiveTime::from_hms_opt(
                                                occurrence_event.start.hour(),
                                                occurrence_event.start.minute(),
                                                0,
                                            )
                                            .unwrap_or_default(),
                                        ),
                                        Some(
                                            chrono::NaiveTime::from_hms_opt(
                                                occurrence_event.end.hour(),
                                                occurrence_event.end.minute(),
                                                0,
                                            )
                                            .unwrap_or_default(),
                                        ),
                                    )
                                };

                                let display_event = DisplayEvent {
                                    calendar_id: source.info().id.clone(),
                                    uid: occurrence_event.uid.clone(),
                                    summary: occurrence_event.summary.clone(),
                                    color: calendar_color.clone(),
                                    all_day: occurrence_event.all_day,
                                    start_time,
                                    end_time,
                                    span_start: None,
                                    span_end: None,
                                };
                                events_by_date
                                    .entry(event_start)
                                    .or_default()
                                    .push(display_event);
                            }
                        }
                    }
                }
            }
        }

        events_by_date
    }

    /// Get events for a specific week grouped by date, with calendar colors.
    /// Returns a HashMap where key is NaiveDate and value is Vec of DisplayEvents.
    pub fn get_display_events_for_week(
        &self,
        week_days: &[chrono::NaiveDate],
    ) -> HashMap<chrono::NaiveDate, Vec<DisplayEvent>> {
        use chrono::NaiveDate;

        let mut events_by_date: HashMap<NaiveDate, Vec<DisplayEvent>> = HashMap::new();

        if week_days.is_empty() {
            return events_by_date;
        }

        let range_start = week_days[0];
        let range_end = week_days[week_days.len() - 1];

        for source in &self.sources {
            if !source.is_enabled() {
                continue;
            }

            let calendar_color = source.info().color.clone();

            if let Ok(events) = source.fetch_events() {
                for event in events {
                    // Expand recurring events into individual occurrences
                    let occurrences = Self::expand_recurring_event(&event, range_start, range_end);

                    for (_occurrence_date, occurrence_event) in occurrences {
                        let event_start = occurrence_event.start.date_naive();
                        let event_end = occurrence_event.end.date_naive();

                        // For all-day/multi-day events, add to each day in the range
                        if occurrence_event.all_day && event_end > event_start {
                            // Multi-day event: iterate through each day
                            let mut current = event_start;
                            while current <= event_end && current <= range_end {
                                if current >= range_start {
                                    let display_event = DisplayEvent {
                                        calendar_id: source.info().id.clone(),
                                        uid: occurrence_event.uid.clone(),
                                        summary: occurrence_event.summary.clone(),
                                        color: calendar_color.clone(),
                                        all_day: true,
                                        start_time: None,
                                        end_time: None,
                                        span_start: Some(event_start),
                                        span_end: Some(event_end),
                                    };
                                    events_by_date
                                        .entry(current)
                                        .or_default()
                                        .push(display_event);
                                }
                                current = current.succ_opt().unwrap_or(current);
                            }
                        } else {
                            // Single-day event: only add to start date
                            if event_start >= range_start && event_start <= range_end {
                                // Extract start and end time for timed events
                                let (start_time, end_time) = if occurrence_event.all_day {
                                    (None, None)
                                } else {
                                    (
                                        Some(
                                            chrono::NaiveTime::from_hms_opt(
                                                occurrence_event.start.hour(),
                                                occurrence_event.start.minute(),
                                                0,
                                            )
                                            .unwrap_or_default(),
                                        ),
                                        Some(
                                            chrono::NaiveTime::from_hms_opt(
                                                occurrence_event.end.hour(),
                                                occurrence_event.end.minute(),
                                                0,
                                            )
                                            .unwrap_or_default(),
                                        ),
                                    )
                                };

                                let display_event = DisplayEvent {
                                    calendar_id: source.info().id.clone(),
                                    uid: occurrence_event.uid.clone(),
                                    summary: occurrence_event.summary.clone(),
                                    color: calendar_color.clone(),
                                    all_day: occurrence_event.all_day,
                                    start_time,
                                    end_time,
                                    span_start: None,
                                    span_end: None,
                                };
                                events_by_date
                                    .entry(event_start)
                                    .or_default()
                                    .push(display_event);
                            }
                        }
                    }
                }
            }
        }

        events_by_date
    }

    /// Sync all calendar sources
    #[allow(dead_code)] // Reserved for future CalDAV sync
    pub fn sync_all(&mut self) -> Result<(), Box<dyn Error>> {
        for source in &mut self.sources {
            if source.is_enabled() {
                source.sync()?;
            }
        }
        Ok(())
    }

    /// Save calendar configuration to config file (not database)
    /// Each calendar's current state (color, enabled, name) is saved
    pub fn save_config(&self) -> Result<(), Box<dyn Error>> {
        let mut config = CalendarManagerConfig::load().unwrap_or_default();

        for source in &self.sources {
            let info = source.info();
            let (server_url, username) = match source.remote_config() {
                Some((url, user)) => (Some(url), Some(user)),
                None => (None, None),
            };
            config.update_calendar(CalendarConfig {
                id: info.id.clone(),
                name: info.name.clone(),
                color: info.color.clone(),
                enabled: info.enabled,
                calendar_type: info.calendar_type.as_config().to_string(),
                server_url,
                username,
            });
        }

        config.save()?;
        Ok(())
    }
}

impl Default for CalendarManager {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caldav::{AlertTime, TravelTime};

    fn custom_event(rrule: &str, exceptions: Vec<NaiveDate>) -> CalendarEvent {
        CalendarEvent {
            uid: "expand-test".to_string(),
            summary: "Expand Test".to_string(),
            location: None,
            all_day: false,
            start: chrono::DateTime::parse_from_rfc3339("2026-09-01T10:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            end: chrono::DateTime::parse_from_rfc3339("2026-09-01T11:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            travel_time: TravelTime::None,
            repeat: RepeatFrequency::Custom(rrule.to_string()),
            repeat_until: None,
            exception_dates: exceptions,
            invitees: vec![],
            alert: AlertTime::None,
            alert_second: None,
            attachments: vec![],
            url: None,
            notes: None,
        }
    }

    /// `FREQ=DAILY;INTERVAL=2` over 2026-09-01..2026-09-09 must yield
    /// 09-01, 09-03, 09-05, 09-07, 09-09 — every other day from the event's
    /// own start date.
    #[test]
    fn test_expand_custom_daily_interval_2() {
        let event = custom_event("FREQ=DAILY;INTERVAL=2", vec![]);
        let range_start = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let range_end = NaiveDate::from_ymd_opt(2026, 9, 9).unwrap();
        let occurrences = CalendarManager::expand_recurring_event(&event, range_start, range_end);
        let dates: Vec<NaiveDate> = occurrences.iter().map(|(d, _)| *d).collect();
        assert_eq!(
            dates,
            vec![
                NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 5).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 7).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 9).unwrap(),
            ]
        );
        // Occurrence times shift with the date, duration preserved.
        let (_, first) = &occurrences[0];
        assert_eq!(
            first.start,
            chrono::DateTime::parse_from_rfc3339("2026-09-01T10:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc)
        );
        assert_eq!(first.end - first.start, chrono::Duration::hours(1));
        // Per-occurrence UIDs are date-suffixed for view dedup.
        assert_eq!(first.uid, "expand-test_20260901");
    }

    /// An exception date removes just that occurrence.
    #[test]
    fn test_expand_custom_skips_exception_dates() {
        let event = custom_event(
            "FREQ=DAILY;INTERVAL=2",
            vec![NaiveDate::from_ymd_opt(2026, 9, 5).unwrap()],
        );
        let range_start = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let range_end = NaiveDate::from_ymd_opt(2026, 9, 9).unwrap();
        let occurrences = CalendarManager::expand_recurring_event(&event, range_start, range_end);
        let dates: Vec<NaiveDate> = occurrences.iter().map(|(d, _)| *d).collect();
        assert_eq!(
            dates,
            vec![
                NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 7).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 9).unwrap(),
            ]
        );
    }

    /// A rule without a FREQ component (e.g. `COUNT=5`) can't be advanced, so
    /// expansion stops after the first occurrence rather than guessing — the
    /// event still shows on its own start date.
    #[test]
    fn test_expand_custom_unmodelled_rule_stops() {
        let event = custom_event("COUNT=5", vec![]);
        let range_start = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let range_end = NaiveDate::from_ymd_opt(2026, 9, 30).unwrap();
        let occurrences = CalendarManager::expand_recurring_event(&event, range_start, range_end);
        assert_eq!(occurrences.len(), 1);
        assert_eq!(
            occurrences[0].0,
            NaiveDate::from_ymd_opt(2026, 9, 1).unwrap()
        );
    }

    /// `FREQ=WEEKLY;BYDAY=MO,WE,FR`: DTSTART is always the first occurrence
    /// (2026-09-01 is a Tuesday), then the listed weekdays in order.
    #[test]
    fn test_expand_custom_weekly_byday() {
        let event = custom_event("FREQ=WEEKLY;BYDAY=MO,WE,FR", vec![]);
        let range_start = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let range_end = NaiveDate::from_ymd_opt(2026, 9, 15).unwrap();
        let occurrences = CalendarManager::expand_recurring_event(&event, range_start, range_end);
        let dates: Vec<NaiveDate> = occurrences.iter().map(|(d, _)| *d).collect();
        assert_eq!(
            dates,
            vec![
                NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(), // DTSTART (Tue)
                NaiveDate::from_ymd_opt(2026, 9, 2).unwrap(), // Wed
                NaiveDate::from_ymd_opt(2026, 9, 4).unwrap(), // Fri
                NaiveDate::from_ymd_opt(2026, 9, 7).unwrap(), // Mon
                NaiveDate::from_ymd_opt(2026, 9, 9).unwrap(), // Wed
                NaiveDate::from_ymd_opt(2026, 9, 11).unwrap(), // Fri
                NaiveDate::from_ymd_opt(2026, 9, 14).unwrap(), // Mon
            ]
        );
    }

    /// `FREQ=WEEKLY;INTERVAL=2;BYDAY=MO`: occurrences only in every second
    /// week, counted from the week containing DTSTART (2026-09-01, a Tuesday,
    /// is in the week of Aug 31). Mondays of weeks 2 and 4: Sep 14, Sep 28.
    #[test]
    fn test_expand_custom_weekly_byday_interval_2() {
        let event = custom_event("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO", vec![]);
        let range_start = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let range_end = NaiveDate::from_ymd_opt(2026, 10, 10).unwrap();
        let occurrences = CalendarManager::expand_recurring_event(&event, range_start, range_end);
        let dates: Vec<NaiveDate> = occurrences.iter().map(|(d, _)| *d).collect();
        assert_eq!(
            dates,
            vec![
                NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(), // DTSTART (Tue)
                NaiveDate::from_ymd_opt(2026, 9, 14).unwrap(), // Mon, week 2
                NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(), // Mon, week 4
            ]
        );
    }

    /// `FREQ=MONTHLY;BYMONTHDAY=15`: DTSTART first, then the 15th of each month.
    #[test]
    fn test_expand_custom_monthly_bymonthday() {
        let event = custom_event("FREQ=MONTHLY;BYMONTHDAY=15", vec![]);
        let range_start = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let range_end = NaiveDate::from_ymd_opt(2026, 11, 30).unwrap();
        let occurrences = CalendarManager::expand_recurring_event(&event, range_start, range_end);
        let dates: Vec<NaiveDate> = occurrences.iter().map(|(d, _)| *d).collect();
        assert_eq!(
            dates,
            vec![
                NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(), // DTSTART
                NaiveDate::from_ymd_opt(2026, 9, 15).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 15).unwrap(),
                NaiveDate::from_ymd_opt(2026, 11, 15).unwrap(),
            ]
        );
    }

    /// `FREQ=YEARLY;BYMONTH=3;BYMONTHDAY=1`: DTSTART first, then Mar 1 each year.
    #[test]
    fn test_expand_custom_yearly_bymonth_bymonthday() {
        let event = custom_event("FREQ=YEARLY;BYMONTH=3;BYMONTHDAY=1", vec![]);
        let range_start = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let range_end = NaiveDate::from_ymd_opt(2027, 6, 30).unwrap();
        let occurrences = CalendarManager::expand_recurring_event(&event, range_start, range_end);
        let dates: Vec<NaiveDate> = occurrences.iter().map(|(d, _)| *d).collect();
        assert_eq!(
            dates,
            vec![
                NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(), // DTSTART
                NaiveDate::from_ymd_opt(2027, 3, 1).unwrap(),
            ]
        );
    }

    /// A daily event whose start is >1000 days before the range must still
    /// expand within the range — expansion fast-forwards instead of iterating
    /// from the start date (which would hit the 1000-iteration cap and return
    /// nothing).
    #[test]
    fn test_expand_fast_forwards_past_iteration_cap() {
        let mut event = custom_event("FREQ=DAILY", vec![]);
        event.repeat = RepeatFrequency::Daily;
        event.start = chrono::DateTime::parse_from_rfc3339("2023-01-01T10:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        event.end = chrono::DateTime::parse_from_rfc3339("2023-01-01T11:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let range_start = NaiveDate::from_ymd_opt(2026, 9, 1).unwrap();
        let range_end = NaiveDate::from_ymd_opt(2026, 9, 5).unwrap();
        let occurrences = CalendarManager::expand_recurring_event(&event, range_start, range_end);
        let dates: Vec<NaiveDate> = occurrences.iter().map(|(d, _)| *d).collect();
        assert_eq!(
            dates,
            vec![
                NaiveDate::from_ymd_opt(2026, 9, 1).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 2).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 4).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 5).unwrap(),
            ]
        );
    }

    /// A weekly event far in the past: occurrences land on the same weekday
    /// as the start date (2023-01-02 is a Monday).
    #[test]
    fn test_expand_weekly_far_future_range() {
        let mut event = custom_event("FREQ=WEEKLY", vec![]);
        event.repeat = RepeatFrequency::Weekly;
        event.start = chrono::DateTime::parse_from_rfc3339("2023-01-02T10:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        event.end = chrono::DateTime::parse_from_rfc3339("2023-01-02T11:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let range_start = NaiveDate::from_ymd_opt(2026, 9, 7).unwrap();
        let range_end = NaiveDate::from_ymd_opt(2026, 9, 28).unwrap();
        let occurrences = CalendarManager::expand_recurring_event(&event, range_start, range_end);
        let dates: Vec<NaiveDate> = occurrences.iter().map(|(d, _)| *d).collect();
        assert_eq!(
            dates,
            vec![
                NaiveDate::from_ymd_opt(2026, 9, 7).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 14).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 21).unwrap(),
                NaiveDate::from_ymd_opt(2026, 9, 28).unwrap(),
            ]
        );
    }
}
