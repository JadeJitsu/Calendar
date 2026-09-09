//! CalDAV client (RFC 4791) for Basic-auth servers: Nextcloud, Radicale,
//! self-hosted, Apple via app-password.
//!
//! Security (repo CLAUDE.md): HTTPS-only, enforced here and at the dialog.
//! Never log credentials, event summaries/notes/locations, invitee emails, or
//! full multistatus/ICS response bodies — log UIDs, calendar IDs, hosts, and
//! status codes only.

use icalendar::{Alarm, Calendar, Component, Event, EventLike, Property, Trigger, ValueType};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::error::Error;

/// Repeat frequency for recurring events
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RepeatFrequency {
    #[default]
    Never,
    Daily,
    Weekly,
    Biweekly,
    Monthly,
    Yearly,
    Custom(String), // For custom RRULE strings
}

/// Alert timing before an event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertTime {
    None,
    AtTime,
    FiveMinutes,
    TenMinutes,
    FifteenMinutes,
    ThirtyMinutes,
    OneHour,
    TwoHours,
    OneDay,
    TwoDays,
    OneWeek,
    Custom(i32), // Custom minutes before
}

impl Default for AlertTime {
    fn default() -> Self {
        AlertTime::None
    }
}

/// Travel time duration options
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TravelTime {
    #[default]
    None,
    FiveMinutes,
    TenMinutes,
    FifteenMinutes,
    ThirtyMinutes,
    FortyFiveMinutes,
    OneHour,
    OneHourThirty,
    TwoHours,
    Custom(i32), // Custom minutes
}

/// A single calendar event, shared between local and CalDAV-backed
/// calendars. This is the app's canonical in-memory event model; the
/// `calendar_event_to_ics` / `parse_ical_string` pair round-trips it to and
/// from iCalendar text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalendarEvent {
    /// Unique identifier for the event
    pub uid: String,
    /// Event title/summary
    pub summary: String,
    /// Event location (address or place name)
    pub location: Option<String>,
    /// Whether this is an all-day event
    pub all_day: bool,
    /// Start date/time
    pub start: chrono::DateTime<chrono::Utc>,
    /// End date/time
    pub end: chrono::DateTime<chrono::Utc>,
    /// Travel time before the event
    pub travel_time: TravelTime,
    /// Repeat/recurrence settings
    pub repeat: RepeatFrequency,
    /// End date for recurring events (None means no end date)
    pub repeat_until: Option<chrono::NaiveDate>,
    /// Exception dates - dates where this recurring event should NOT appear
    /// Used when deleting a single occurrence of a recurring event
    pub exception_dates: Vec<chrono::NaiveDate>,
    /// Invitees (email addresses)
    pub invitees: Vec<String>,
    /// Alert/reminder settings
    pub alert: AlertTime,
    /// Second alert (optional)
    pub alert_second: Option<AlertTime>,
    /// File attachments (paths or URLs)
    pub attachments: Vec<String>,
    /// URL associated with the event
    pub url: Option<String>,
    /// Notes/description
    pub notes: Option<String>,
}

/// A calendar collection discovered via PROPFIND.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveredCalendar {
    pub name: String,
    /// The collection URL (absolute) to REPORT against.
    pub url: String,
    /// Server-suggested color, if any.
    pub color: Option<String>,
}

/// Errors from CalDAV operations. Messages are safe to log (no secrets,
/// no event bodies).
#[derive(Debug)]
pub enum CalDavError {
    /// The server URL is not HTTPS.
    NotHttps,
    /// A network/transport failure.
    Http(reqwest::Error),
    /// The server returned a non-success status.
    Status(reqwest::StatusCode),
    /// A response could not be parsed.
    Parse(String),
}

impl std::fmt::Display for CalDavError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CalDavError::NotHttps => write!(f, "CalDAV server URL must use HTTPS"),
            CalDavError::Http(e) => write!(f, "CalDAV request failed: {e}"),
            CalDavError::Status(s) => write!(f, "CalDAV server returned {s}"),
            CalDavError::Parse(msg) => write!(f, "CalDAV response parse error: {msg}"),
        }
    }
}

impl Error for CalDavError {}

impl From<reqwest::Error> for CalDavError {
    fn from(e: reqwest::Error) -> Self {
        CalDavError::Http(e)
    }
}

/// CalDAV client. `Send + Clone` — safe to clone into `Task::perform` for
/// network work off the UI thread.
#[derive(Debug, Clone)]
pub struct CalDavClient {
    base_url: String,
    username: String,
    password: String,
    client: Client,
}

impl CalDavClient {
    /// Create a client for `base_url`. Fails with [`CalDavError::NotHttps`]
    /// if the URL is not `https://` — plaintext CalDAV is refused outright.
    pub fn new(base_url: String, username: String, password: String) -> Result<Self, CalDavError> {
        // Security: enforce HTTPS-only connections
        if !base_url.starts_with("https://") {
            return Err(CalDavError::NotHttps);
        }

        let client = Client::builder()
            .https_only(true)
            .build()
            .map_err(CalDavError::Http)?;

        Ok(CalDavClient {
            base_url,
            username,
            password,
            client,
        })
    }

    /// The server host (safe to log).
    pub fn host(&self) -> String {
        url::Url::parse(&self.base_url)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_string()))
            .unwrap_or_else(|| self.base_url.clone())
    }

    /// The base URL (no trailing slash).
    pub fn base_url_public(&self) -> &str {
        &self.base_url
    }

    /// The username (safe to log).
    pub fn username_public(&self) -> &str {
        &self.username
    }

    // ── PROPFIND discovery ────────────────────────────────────────────────

    /// Send a PROPFIND and return the response body.
    fn propfind(&self, url: &str, depth: &str, body: &str) -> Result<String, CalDavError> {
        let method = Self::method("PROPFIND")?;
        let response = self
            .client
            .request(method, url)
            .header("Depth", depth)
            .header("Content-Type", "application/xml; charset=utf-8")
            .basic_auth(&self.username, Some(&self.password))
            .body(body.to_string())
            .send()?;

        let status = response.status();
        if !status.is_success() {
            return Err(CalDavError::Status(status));
        }
        response.text().map_err(CalDavError::Http)
    }

    /// Resolve the current-user-principal URL (PROPFIND A).
    pub fn discover_principal(&self) -> Result<String, CalDavError> {
        let url = self.base_url();
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<d:prop xmlns:d="DAV:">
    <d:current-user-principal/>
</d:prop>"#;
        let xml = self.propfind(&url, "0", body)?;
        let doc = roxmltree::Document::parse(&xml).map_err(|e| CalDavError::Parse(e.to_string()))?;
        let href = doc
            .descendants()
            .find(|e| e.has_tag_name("current-user-principal"))
            .and_then(|e| e.first_child())
            .filter(|e| e.is_element())
            .filter(|e| e.has_tag_name("href"))
            .and_then(|e| e.text())
            .ok_or_else(|| CalDavError::Parse("no current-user-principal href".into()))?;
        Ok(Self::resolve_href(&url, href))
    }

    /// Resolve the calendar-home-set URL (PROPFIND B).
    pub fn discover_home(&self, principal: &str) -> Result<String, CalDavError> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<d:prop xmlns:d="DAV:">
    <d:calendar-home-set/>
</d:prop>"#;
        let xml = self.propfind(principal, "0", body)?;
        let doc = roxmltree::Document::parse(&xml).map_err(|e| CalDavError::Parse(e.to_string()))?;
        let href = doc
            .descendants()
            .find(|e| e.has_tag_name("calendar-home-set"))
            .and_then(|e| e.first_child())
            .filter(|e| e.is_element())
            .filter(|e| e.has_tag_name("href"))
            .and_then(|e| e.text())
            .ok_or_else(|| CalDavError::Parse("no calendar-home-set href".into()))?;
        Ok(Self::resolve_href(principal, href))
    }

    /// List calendar collections under a home-set URL (PROPFIND C).
    pub fn list_calendars(&self, home: &str) -> Result<Vec<DiscoveredCalendar>, CalDavError> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?>
<d:prop xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
    <d:displayname/>
    <c:calendar-color/>
    <d:resourcetype/>
</d:prop>"#;
        let xml = self.propfind(home, "1", body)?;
        Ok(parse_calendar_multistatus(&xml, home)?)
    }

    /// Full discovery: principal → home-set → calendar list. If any step
    /// fails (e.g. the pasted URL is already a collection), fall back to
    /// treating the base URL itself as a single calendar.
    pub fn discover(&self) -> Result<Vec<DiscoveredCalendar>, CalDavError> {
        let principal = self.discover_principal()?;
        let home = self.discover_home(&principal)?;
        let calendars = self.list_calendars(&home)?;
        if !calendars.is_empty() {
            return Ok(calendars);
        }
        Err(CalDavError::Parse("no calendars found".into()))
    }

    /// The fallback: treat the base URL as a single calendar collection.
    pub fn single_calendar(&self) -> DiscoveredCalendar {
        DiscoveredCalendar {
            name: self.host(),
            url: self.base_url(),
            color: None,
        }
    }

    // ── Event fetch ───────────────────────────────────────────────────────

    /// REPORT a calendar collection. Returns `(href, ics_body)` pairs — the
    /// caller parses the ICS (shared parser) and keeps the uid→href map.
    pub fn fetch_events(&self, collection_url: &str) -> Result<Vec<(String, String)>, CalDavError> {
        let caldav_query = r#"<?xml version="1.0" encoding="utf-8" ?>
<C:calendar-query xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
    <D:prop>
        <D:getetag/>
        <C:calendar-data/>
    </D:prop>
    <C:filter>
        <C:comp-filter name="VCALENDAR">
            <C:comp-filter name="VEVENT"/>
        </C:comp-filter>
    </C:filter>
</C:calendar-query>"#;

        let method = Self::method("REPORT")?;
        let response = self
            .client
            .request(method, collection_url)
            .header("Depth", "1")
            .header("Content-Type", "application/xml; charset=utf-8")
            .basic_auth(&self.username, Some(&self.password))
            .body(caldav_query.to_string())
            .send()?;

        let status = response.status();
        if !status.is_success() {
            return Err(CalDavError::Status(status));
        }

        let body = response.text().map_err(CalDavError::Http)?;
        Ok(parse_event_multistatus(&body, collection_url)?)
    }

    // ── Event write-back ──────────────────────────────────────────────────

    /// PUT an event to its href. Returns the ETag of the stored resource.
    pub fn put_event(
        &self,
        href: &str,
        ics: &str,
        if_match: Option<&str>,
    ) -> Result<String, CalDavError> {
        let mut req = self
            .client
            .put(href)
            .header("Content-Type", "text/calendar; charset=utf-8")
            .basic_auth(&self.username, Some(&self.password))
            .body(ics.to_string());
        if let Some(etag) = if_match {
            req = req.header("If-Match", etag);
        }
        let response = req.send()?;
        let status = response.status();
        if !status.is_success() {
            return Err(CalDavError::Status(status));
        }
        Ok(response
            .headers()
            .get("ETag")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string())
    }

    /// DELETE an event by href.
    pub fn delete_event(&self, href: &str, if_match: Option<&str>) -> Result<(), CalDavError> {
        let mut req = self
            .client
            .delete(href)
            .basic_auth(&self.username, Some(&self.password));
        if let Some(etag) = if_match {
            req = req.header("If-Match", etag);
        }
        let response = req.send()?;
        let status = response.status();
        if !status.is_success() {
            return Err(CalDavError::Status(status));
        }
        Ok(())
    }

    // ── helpers ───────────────────────────────────────────────────────────

    /// Build a non-standard HTTP method (PROPFIND/REPORT) that `http` doesn't
    /// expose as a constant.
    fn method(name: &str) -> Result<reqwest::Method, CalDavError> {
        reqwest::Method::from_bytes(name.as_bytes()).map_err(|_| CalDavError::Parse(format!("invalid method {name}")))
    }

    /// The base URL with a trailing slash (for joining relative hrefs).
    fn base_url(&self) -> String {
        if self.base_url.ends_with('/') {
            self.base_url.clone()
        } else {
            format!("{}/", self.base_url)
        }
    }

    /// Resolve a (possibly relative) href from a PROPFIND/REPORT response
    /// against the request URL it came from.
    fn resolve_href(request_url: &str, href: &str) -> String {
        url::Url::parse(request_url)
            .and_then(|u| u.join(href))
            .map(|u| u.to_string())
            .unwrap_or_else(|_| href.to_string())
    }
}

// ── multistatus parsing (pure, testable) ────────────────────────────────────

/// Parse a Depth-1 calendar-list multistatus into discovered calendars.
/// Keeps only responses whose `resourcetype` contains a `calendar` element.
pub fn parse_calendar_multistatus(xml: &str, base_url: &str) -> Result<Vec<DiscoveredCalendar>, CalDavError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| CalDavError::Parse(e.to_string()))?;
    let mut out = Vec::new();
    for resp in doc.descendants().filter(|e| e.has_tag_name("response")) {
        let href = resp
            .descendants()
            .find(|e| e.has_tag_name("href"))
            .and_then(|e| e.text())
            .ok_or_else(|| CalDavError::Parse("response without href".into()))?;
        let is_calendar = resp
            .descendants()
            .find(|e| e.has_tag_name("resourcetype"))
            .map(|rt| rt.descendants().any(|e| e.has_tag_name("calendar")))
            .unwrap_or(false);
        if !is_calendar {
            continue;
        }
        let name = resp
            .descendants()
            .find(|e| e.has_tag_name("displayname"))
            .and_then(|e| e.text())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| CalDavClient::resolve_href(base_url, href));
        let color = resp
            .descendants()
            .find(|e| e.has_tag_name("calendar-color"))
            .and_then(|e| e.text())
            .map(|s| s.to_string());
        out.push(DiscoveredCalendar {
            name,
            url: CalDavClient::resolve_href(base_url, href),
            color,
        });
    }
    Ok(out)
}

/// Parse a REPORT calendar-query multistatus into `(href, ics_body)` pairs.
pub fn parse_event_multistatus(xml: &str, base_url: &str) -> Result<Vec<(String, String)>, CalDavError> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| CalDavError::Parse(e.to_string()))?;
    let mut out = Vec::new();
    for resp in doc.descendants().filter(|e| e.has_tag_name("response")) {
        let href = resp
            .descendants()
            .find(|e| e.has_tag_name("href"))
            .and_then(|e| e.text())
            .ok_or_else(|| CalDavError::Parse("response without href".into()))?;
        let ics = resp
            .descendants()
            .find(|e| e.has_tag_name("calendar-data"))
            .and_then(|e| e.text())
            .ok_or_else(|| CalDavError::Parse("response without calendar-data".into()))?;
        out.push((CalDavClient::resolve_href(base_url, href), ics.to_string()));
    }
    Ok(out)
}

// ── outbound ICS ────────────────────────────────────────────────────────────

/// Map a `RepeatFrequency` to an RRULE string (inverse of the inbound
/// `rrule_to_repeat` mapping). Returns `None` for `Never`.
pub fn repeat_to_rrule(repeat: &RepeatFrequency, repeat_until: Option<chrono::NaiveDate>) -> Option<String> {
    let base = match repeat {
        RepeatFrequency::Never => return None,
        RepeatFrequency::Daily => "FREQ=DAILY",
        RepeatFrequency::Weekly => "FREQ=WEEKLY",
        RepeatFrequency::Biweekly => "FREQ=WEEKLY;INTERVAL=2",
        RepeatFrequency::Monthly => "FREQ=MONTHLY",
        RepeatFrequency::Yearly => "FREQ=YEARLY",
        RepeatFrequency::Custom(s) => s.as_str(),
    };
    match repeat_until {
        Some(until) => Some(format!(
            "{base};UNTIL={}",
            until.format("%Y%m%dT000000Z")
        )),
        None => Some(base.to_string()),
    }
}

/// Map an `AlertTime` to an iCalendar TRIGGER (negative duration = before start).
pub fn alert_to_trigger(alert: &AlertTime) -> Option<Trigger> {
    let duration = match alert {
        AlertTime::None | AlertTime::AtTime => return None,
        AlertTime::FiveMinutes => chrono::Duration::minutes(5),
        AlertTime::TenMinutes => chrono::Duration::minutes(10),
        AlertTime::FifteenMinutes => chrono::Duration::minutes(15),
        AlertTime::ThirtyMinutes => chrono::Duration::minutes(30),
        AlertTime::OneHour => chrono::Duration::hours(1),
        AlertTime::TwoHours => chrono::Duration::hours(2),
        AlertTime::OneDay => chrono::Duration::days(1),
        AlertTime::TwoDays => chrono::Duration::days(2),
        AlertTime::OneWeek => chrono::Duration::weeks(1),
        AlertTime::Custom(mins) => chrono::Duration::minutes(*mins as i64),
    };
    Some(Trigger::before_start(duration))
}

/// Serialize a `CalendarEvent` to a VCALENDAR string for PUT.
///
/// icalendar 0.16 quirks handled here:
/// - no RRULE emitter → `add_property("RRULE", ...)`
/// - no VTIMEZONE emitter → timed events emit UTC `Z` (the model stores
///   `DateTime<Utc>`), all-day events emit `VALUE=DATE`
/// - `all_day()` sets DTSTART and DTEND to the *same* date, so a multi-day
///   all-day event gets a manual DTEND overwrite
pub fn calendar_event_to_ics(event: &CalendarEvent) -> String {
    let mut calendar = Calendar::new();
    let mut ev = Event::new();

    ev.uid(&event.uid);
    ev.summary(&event.summary);
    ev.timestamp(chrono::Utc::now()); // DTSTAMP

    if let Some(notes) = &event.notes {
        ev.description(notes);
    }
    if let Some(loc) = &event.location {
        ev.location(loc);
    }
    if let Some(url) = &event.url {
        ev.url(url);
    }

    if event.all_day {
        ev.all_day(event.start.date_naive());
        // all_day() sets DTEND == DTSTART; extend for multi-day events.
        if event.end.date_naive() != event.start.date_naive() {
            ev.append_property(
                Property::new("DTEND", event.end.date_naive().format("%Y%m%d").to_string())
                    .append_parameter(ValueType::Date)
                    .done(),
            );
        }
    } else {
        ev.starts(event.start);
        ev.ends(event.end);
    }

    if let Some(rrule) = repeat_to_rrule(&event.repeat, event.repeat_until) {
        ev.add_property("RRULE", &rrule);
    }
    for d in &event.exception_dates {
        ev.append_multi_property(Property::new(
            "EXDATE",
            &d.format("%Y%m%d").to_string(),
        ));
    }
    if let Some(trigger) = alert_to_trigger(&event.alert) {
        ev.alarm(Alarm::display("Reminder", trigger));
    }

    calendar.push(ev);
    calendar.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> CalendarEvent {
        CalendarEvent {
            uid: "test-event-1".to_string(),
            summary: "Test Event".to_string(),
            location: Some("Test Location".to_string()),
            all_day: false,
            start: chrono::Utc::now(),
            end: chrono::Utc::now() + chrono::Duration::hours(1),
            travel_time: TravelTime::None,
            repeat: RepeatFrequency::Never,
            repeat_until: None,
            exception_dates: vec![],
            invitees: vec![],
            alert: AlertTime::FifteenMinutes,
            alert_second: None,
            attachments: vec![],
            url: None,
            notes: Some("A test event".to_string()),
        }
    }

    #[test]
    fn test_client_rejects_http() {
        assert!(matches!(
            CalDavClient::new("http://example.com/caldav".into(), "u".into(), "p".into()),
            Err(CalDavError::NotHttps)
        ));
    }

    #[test]
    fn test_client_accepts_https() {
        CalDavClient::new("https://example.com/caldav".into(), "u".into(), "p".into())
            .expect("https client should build");
    }

    #[test]
    fn test_repeat_to_rrule() {
        assert_eq!(repeat_to_rrule(&RepeatFrequency::Never, None), None);
        assert_eq!(repeat_to_rrule(&RepeatFrequency::Weekly, None).as_deref(), Some("FREQ=WEEKLY"));
        assert_eq!(
            repeat_to_rrule(&RepeatFrequency::Biweekly, None).as_deref(),
            Some("FREQ=WEEKLY;INTERVAL=2")
        );
        let until = chrono::NaiveDate::from_ymd_opt(2026, 12, 25).unwrap();
        assert_eq!(
            repeat_to_rrule(&RepeatFrequency::Monthly, Some(until)).as_deref(),
            Some("FREQ=MONTHLY;UNTIL=20261225T000000Z")
        );
        assert_eq!(
            repeat_to_rrule(&RepeatFrequency::Custom("FREQ=DAILY;BYDAY=MO".into()), None).as_deref(),
            Some("FREQ=DAILY;BYDAY=MO")
        );
    }

    #[test]
    fn test_alert_to_trigger() {
        assert_eq!(alert_to_trigger(&AlertTime::None), None);
        assert_eq!(
            alert_to_trigger(&AlertTime::FifteenMinutes),
            Some(Trigger::before_start(chrono::Duration::minutes(15)))
        );
        assert_eq!(
            alert_to_trigger(&AlertTime::OneDay),
            Some(Trigger::before_start(chrono::Duration::days(1)))
        );
        assert_eq!(
            alert_to_trigger(&AlertTime::Custom(7)),
            Some(Trigger::before_start(chrono::Duration::minutes(7)))
        );
    }

    #[test]
    fn test_ics_round_trip_fields() {
        let mut event = sample_event();
        event.repeat = RepeatFrequency::Biweekly;
        event.exception_dates = vec![chrono::NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()];

        let ics = calendar_event_to_ics(&event);
        assert!(ics.contains("UID:test-event-1"));
        assert!(ics.contains("SUMMARY:Test Event"));
        assert!(ics.contains("LOCATION:Test Location"));
        assert!(ics.contains("DESCRIPTION:A test event"));
        assert!(ics.contains("DTSTAMP:"));
        assert!(ics.contains("RRULE:FREQ=WEEKLY;INTERVAL=2"));
        assert!(ics.contains("EXDATE:20261001"));
        assert!(ics.contains("BEGIN:VALARM"));
        assert!(ics.contains("TRIGGER;RELATED=START:-PT900S"));
    }

    #[test]
    fn test_ics_all_day() {
        let event = CalendarEvent {
            uid: "allday-1".to_string(),
            summary: "All day".to_string(),
            location: None,
            all_day: true,
            start: chrono::DateTime::parse_from_rfc3339("2026-09-10T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            end: chrono::DateTime::parse_from_rfc3339("2026-09-12T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            travel_time: TravelTime::None,
            repeat: RepeatFrequency::Never,
            repeat_until: None,
            exception_dates: vec![],
            invitees: vec![],
            alert: AlertTime::None,
            alert_second: None,
            attachments: vec![],
            url: None,
            notes: None,
        };
        let ics = calendar_event_to_ics(&event);
        assert!(ics.contains("DTSTART;VALUE=DATE:20260910"));
        // multi-day: DTEND must be the end date, not the start date
        assert!(ics.contains("DTEND;VALUE=DATE:20260912"));
    }

    #[test]
    fn test_parse_calendar_multistatus() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/calendars/user/home/</d:href>
    <d:propstat><d:prop>
      <d:displayname>My Home</d:displayname>
      <d:resourcetype><c:calendar/></d:resourcetype>
    </d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/calendars/user/work/</d:href>
    <d:propstat><d:prop>
      <d:displayname>Work</d:displayname>
      <c:calendar-color>#ff0000</c:calendar-color>
      <d:resourcetype><c:calendar/></d:resourcetype>
    </d:prop></d:propstat>
  </d:response>
  <d:response>
    <d:href>/calendars/user/inbox/</d:href>
    <d:propstat><d:prop>
      <d:displayname>Inbox</d:displayname>
      <d:resourcetype/>
    </d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;
        let cal = parse_calendar_multistatus(xml, "https://example.com/caldav/")
            .expect("parse");
        assert_eq!(cal.len(), 2);
        assert_eq!(cal[0].name, "My Home");
        assert_eq!(cal[0].url, "https://example.com/calendars/user/home/");
        assert_eq!(cal[1].name, "Work");
        assert_eq!(cal[1].color.as_deref(), Some("#ff0000"));
    }

    #[test]
    fn test_parse_event_multistatus() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:response>
    <d:href>/calendars/user/work/evt-1.ics</d:href>
    <d:propstat><d:prop>
      <d:getetag>"abc123"</d:getetag>
      <c:calendar-data>BEGIN:VCALENDAR
VERSION:2.0
END:VCALENDAR</c:calendar-data>
    </d:prop></d:propstat>
  </d:response>
</d:multistatus>"#;
        let events = parse_event_multistatus(xml, "https://example.com/caldav/")
            .expect("parse");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].0, "https://example.com/calendars/user/work/evt-1.ics");
        assert!(events[0].1.contains("BEGIN:VCALENDAR"));
    }

    /// True round-trip: `calendar_event_to_ics` → `parse_ical_string` and
    /// assert the model fields survive, not just that the ICS text contains
    /// the right lines.
    #[test]
    fn test_ics_round_trip_timed_recurring() {
        let event = CalendarEvent {
            uid: "rt-timed-1".to_string(),
            summary: "Round Trip Timed".to_string(),
            location: Some("RT Location".to_string()),
            all_day: false,
            start: chrono::DateTime::parse_from_rfc3339("2026-09-10T09:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            end: chrono::DateTime::parse_from_rfc3339("2026-09-10T10:30:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            travel_time: TravelTime::None,
            repeat: RepeatFrequency::Biweekly,
            repeat_until: None,
            exception_dates: vec![chrono::NaiveDate::from_ymd_opt(2026, 9, 24).unwrap()],
            invitees: vec![],
            alert: AlertTime::FifteenMinutes,
            alert_second: None,
            attachments: vec![],
            url: None,
            notes: Some("RT notes".to_string()),
        };

        let ics = calendar_event_to_ics(&event);
        let parsed = crate::services::ExportHandler::parse_ical_string(&ics)
            .expect("parse back")
            .pop()
            .expect("one event");

        assert_eq!(parsed.uid, "rt-timed-1");
        assert_eq!(parsed.summary, "Round Trip Timed");
        assert_eq!(parsed.location.as_deref(), Some("RT Location"));
        assert_eq!(parsed.notes.as_deref(), Some("RT notes"));
        assert!(!parsed.all_day);
        // Timed events emit UTC `Z` and must parse back to the same instants.
        assert_eq!(
            parsed.start,
            chrono::DateTime::parse_from_rfc3339("2026-09-10T09:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc)
        );
        assert_eq!(
            parsed.end,
            chrono::DateTime::parse_from_rfc3339("2026-09-10T10:30:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc)
        );
        // RRULE:FREQ=WEEKLY;INTERVAL=2 must map back to Biweekly.
        assert_eq!(parsed.repeat, RepeatFrequency::Biweekly);
        assert_eq!(parsed.repeat_until, None);
        assert_eq!(
            parsed.exception_dates,
            vec![chrono::NaiveDate::from_ymd_opt(2026, 9, 24).unwrap()]
        );
        assert_eq!(parsed.alert, AlertTime::FifteenMinutes);
    }

    #[test]
    fn test_ics_round_trip_all_day_multi_day() {
        let event = CalendarEvent {
            uid: "rt-allday-1".to_string(),
            summary: "Round Trip All Day".to_string(),
            location: None,
            all_day: true,
            start: chrono::DateTime::parse_from_rfc3339("2026-09-10T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            end: chrono::DateTime::parse_from_rfc3339("2026-09-12T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            travel_time: TravelTime::None,
            repeat: RepeatFrequency::Never,
            repeat_until: None,
            exception_dates: vec![],
            invitees: vec![],
            alert: AlertTime::None,
            alert_second: None,
            attachments: vec![],
            url: None,
            notes: None,
        };

        let ics = calendar_event_to_ics(&event);
        let parsed = crate::services::ExportHandler::parse_ical_string(&ics)
            .expect("parse back")
            .pop()
            .expect("one event");

        assert!(parsed.all_day);
        assert_eq!(
            parsed.start,
            chrono::DateTime::parse_from_rfc3339("2026-09-10T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc)
        );
        // Multi-day DTEND must survive, not collapse onto DTSTART.
        assert_eq!(
            parsed.end,
            chrono::DateTime::parse_from_rfc3339("2026-09-12T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc)
        );
    }
}
