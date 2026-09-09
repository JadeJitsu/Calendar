//! CalDAV client (RFC 4791) for Basic-auth servers: Nextcloud, Radicale,
//! self-hosted, Apple via app-password.
//!
//! Security (repo CLAUDE.md): HTTPS-only, enforced here and at the dialog.
//! Never log credentials, event summaries/notes/locations, invitee emails, or
//! full multistatus/ICS response bodies — log UIDs, calendar IDs, hosts, and
//! status codes only.

use icalendar::{Alarm, Component, Event, EventLike, Property, Trigger, ValueType};
#[cfg(test)]
use icalendar::Calendar;
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

    /// PROPFIND body for `current-user-principal`.
    ///
    /// RFC 4918 §9.1: the root element must be `DAV:propfind` wrapping
    /// `DAV:prop`. SabreDAV (Nextcloud) rejects a bare `DAV:prop` root with
    /// 400 "Expected {DAV:}propfind but received {DAV:}prop"; lenient
    /// servers (Apache mod_dav) accept both.
    const PRINCIPAL_PROPFIND: &'static str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:">
    <d:prop>
        <d:current-user-principal/>
    </d:prop>
</d:propfind>"#;

    /// PROPFIND body for `calendar-home-set` (same `DAV:propfind` root
    /// requirement as [`Self::PRINCIPAL_PROPFIND`]).
    const HOME_PROPFIND: &'static str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:">
    <d:prop>
        <d:calendar-home-set/>
    </d:prop>
</d:propfind>"#;

    /// PROPFIND body listing calendar collections (same `DAV:propfind` root
    /// requirement as [`Self::PRINCIPAL_PROPFIND`]).
    const LIST_CAL_PROPFIND: &'static str = r#"<?xml version="1.0" encoding="utf-8"?>
<d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
    <d:prop>
        <d:displayname/>
        <c:calendar-color/>
        <d:resourcetype/>
    </d:prop>
</d:propfind>"#;

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
        let xml = self.propfind(&url, "0", Self::PRINCIPAL_PROPFIND)?;
        let doc = roxmltree::Document::parse(&xml).map_err(|e| CalDavError::Parse(e.to_string()))?;
        let href = propstat_href(&doc, "current-user-principal")?;
        Ok(Self::resolve_href(&url, &href))
    }

    /// Resolve the calendar-home-set URL (PROPFIND B).
    pub fn discover_home(&self, principal: &str) -> Result<String, CalDavError> {
        let xml = self.propfind(principal, "0", Self::HOME_PROPFIND)?;
        let doc = roxmltree::Document::parse(&xml).map_err(|e| CalDavError::Parse(e.to_string()))?;
        let href = propstat_href(&doc, "calendar-home-set")?;
        Ok(Self::resolve_href(principal, &href))
    }

    /// List calendar collections under a home-set URL (PROPFIND C).
    pub fn list_calendars(&self, home: &str) -> Result<Vec<DiscoveredCalendar>, CalDavError> {
        let xml = self.propfind(home, "1", Self::LIST_CAL_PROPFIND)?;
        Ok(parse_calendar_multistatus(&xml, home)?)
    }

    /// Full discovery: principal → home-set → calendar list.
    ///
    /// Fallback: some servers (observed on Nextcloud 34) do not expose
    /// `calendar-home-set` on the principal — the property 404s even
    /// though the per-user collections exist and are listable directly.
    /// When the home-set step fails, list `{base}/calendars/{username}/`
    /// instead, which is the path direct-URL clients (e.g. Thunderbird)
    /// use.
    pub fn discover(&self) -> Result<Vec<DiscoveredCalendar>, CalDavError> {
        let principal = self.discover_principal()?;
        let home = match self.discover_home(&principal) {
            Ok(h) => h,
            Err(e) => {
                // The per-user path segment is the user's UID — the last
                // path component of the principal URL (Nextcloud:
                // `/principals/users/{uid}/`). Fall back to the login name
                // if the URL doesn't match that shape.
                let uid = url::Url::parse(&principal)
                    .ok()
                    .and_then(|u| {
                        u.path()
                            .rsplit('/')
                            .find(|s| !s.is_empty())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_else(|| self.username.clone());
                let user_url = Self::user_calendars_url(&self.base_url(), &uid);
                match self.list_calendars(&user_url) {
                    Ok(cals) if !cals.is_empty() => {
                        log::warn!(
                            "CalDAV: calendar-home-set unavailable ({}); \
                             falling back to per-user path {}",
                            e,
                            user_url
                        );
                        return Ok(cals);
                    }
                    _ => return Err(e),
                }
            }
        };
        let calendars = self.list_calendars(&home)?;
        if !calendars.is_empty() {
            return Ok(calendars);
        }
        Err(CalDavError::Parse("no calendars found".into()))
    }

    /// The per-user DAV collection path: `{base}/calendars/{username}/`.
    /// Trailing-slash-insensitive on `base_url`.
    pub fn user_calendars_url(base_url: &str, username: &str) -> String {
        format!(
            "{}/calendars/{}/",
            base_url.trim_end_matches('/'),
            username
        )
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

/// Extract the `d:href` child of a single-property PROPFIND response,
/// reporting the server's propstat status when the property was not
/// returned successfully.
///
/// A 207 Multi-Status body can still mean "property not found": each
/// `d:propstat` carries its own `d:status`. Nextcloud answers a missing
/// `calendar-home-set` (CalDAV app not enabled) with
/// `HTTP/1.1 404 Not Found` and an empty property element — without
/// checking the status, that surfaces as the misleading "no X href"
/// parse error.
fn propstat_href(doc: &roxmltree::Document, prop: &str) -> Result<String, CalDavError> {
    let prop_el = doc
        .descendants()
        .find(|e| e.has_tag_name(prop))
        .ok_or_else(|| CalDavError::Parse(format!("no {prop} element in response")))?;

    // The href, if the server returned the property at all. (Descendant
    // search, not first-child: responses may carry whitespace text nodes
    // between the property element and its href.)
    if let Some(href) = prop_el
        .descendants()
        .find(|e| e.has_tag_name("href"))
        .and_then(|e| e.text())
    {
        return Ok(href.to_string());
    }

    // No href: find the enclosing propstat's status and report it.
    let status = prop_el
        .ancestors()
        .find(|e| e.has_tag_name("propstat"))
        .and_then(|ps| {
            ps.children()
                .find(|e| e.is_element() && e.has_tag_name("status"))
                .and_then(|e| e.text())
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "unknown status".to_string());

    Err(CalDavError::Parse(format!(
        "server did not return {prop}: {status} (on Nextcloud this usually means the CalDAV app is not enabled)"
    )))
}

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

/// Resolve a user-authored `Custom` RRULE for storage. An empty or
/// malformed rule (must start with `FREQ=`) is rejected and downgraded to
/// `Never` rather than stored, so a broken rule can't silently produce a
/// non-recurring event with a dangling RRULE.
pub fn resolve_custom_rrule(repeat: &RepeatFrequency) -> RepeatFrequency {
    match repeat {
        RepeatFrequency::Custom(rrule)
            if rrule.trim().is_empty()
                || !rrule.trim().to_ascii_uppercase().starts_with("FREQ=") =>
        {
            RepeatFrequency::Never
        }
        other => other.clone(),
    }
}

/// Map an `AlertTime` to minutes before the event start, or `None` when the
/// alert never fires on its own (`None` / `AtTime` — the latter needs the
/// event's start instant, which the scheduler resolves separately).
pub fn alert_minutes(alert: &AlertTime) -> Option<i64> {
    match alert {
        AlertTime::None | AlertTime::AtTime => None,
        AlertTime::FiveMinutes => Some(5),
        AlertTime::TenMinutes => Some(10),
        AlertTime::FifteenMinutes => Some(15),
        AlertTime::ThirtyMinutes => Some(30),
        AlertTime::OneHour => Some(60),
        AlertTime::TwoHours => Some(120),
        AlertTime::OneDay => Some(1440),
        AlertTime::TwoDays => Some(2880),
        AlertTime::OneWeek => Some(10080),
        AlertTime::Custom(mins) => Some(*mins as i64),
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

/// Build a full VEVENT for a `CalendarEvent`. This is the single source of
/// truth for event serialization — both the test-only `calendar_event_to_ics`
/// and the live CalDAV write path (`ExportHandler::event_to_ical`) use it, so
/// recurrence/exception/reminder/attendee/all-day can't drift between them.
///
/// icalendar 0.16 quirks handled here:
/// - no RRULE emitter → `add_property("RRULE", ...)`
/// - no VTIMEZONE emitter → timed events emit UTC `Z` (the model stores
///   `DateTime<Utc>`), all-day events emit `VALUE=DATE`
/// - `all_day()` sets DTSTART and DTEND to the *same* date, so a multi-day
///   all-day event gets a manual DTEND overwrite
pub fn build_event(event: &CalendarEvent) -> Event {
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
    // ATTENDEE is multi-valued (icalendar routes it to `multi_properties`).
    // We only store bare emails, so emit `mailto:` with no `CN` display name.
    for invitee in &event.invitees {
        ev.append_multi_property(Property::new(
            "ATTENDEE",
            &format!("mailto:{}", invitee),
        ));
    }
    if let Some(trigger) = alert_to_trigger(&event.alert) {
        ev.alarm(Alarm::display("Reminder", trigger));
    }

    ev
}

/// Serialize a `CalendarEvent` to a VCALENDAR string for PUT.
///
/// Test-only helper — the live write path is `ExportHandler::event_to_ical`,
/// which delegates to [`build_event`] directly. Kept for the round-trip tests
/// in this module.
#[cfg(test)]
pub fn calendar_event_to_ics(event: &CalendarEvent) -> String {
    let mut calendar = Calendar::new();
    calendar.push(build_event(event));
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
    fn test_resolve_custom_rrule() {
        // A well-formed custom rule is kept as-is.
        assert_eq!(
            resolve_custom_rrule(&RepeatFrequency::Custom("FREQ=WEEKLY;BYDAY=MO".into())),
            RepeatFrequency::Custom("FREQ=WEEKLY;BYDAY=MO".into())
        );
        // Empty and whitespace-only rules downgrade to Never.
        assert_eq!(resolve_custom_rrule(&RepeatFrequency::Custom("".into())), RepeatFrequency::Never);
        assert_eq!(
            resolve_custom_rrule(&RepeatFrequency::Custom("   ".into())),
            RepeatFrequency::Never
        );
        // A rule without FREQ= is malformed -> Never.
        assert_eq!(
            resolve_custom_rrule(&RepeatFrequency::Custom("INTERVAL=2".into())),
            RepeatFrequency::Never
        );
        // Case-insensitive FREQ prefix.
        assert_eq!(
            resolve_custom_rrule(&RepeatFrequency::Custom("freq=daily".into())),
            RepeatFrequency::Custom("freq=daily".into())
        );
        // Non-custom variants pass through untouched.
        assert_eq!(resolve_custom_rrule(&RepeatFrequency::Weekly), RepeatFrequency::Weekly);
        assert_eq!(resolve_custom_rrule(&RepeatFrequency::Never), RepeatFrequency::Never);
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

    /// Invitees must survive the ICS round-trip: the export writes one
    /// `ATTENDEE` per invitee and the import reads them back.
    #[test]
    fn test_ics_round_trip_invitees() {
        let event = CalendarEvent {
            uid: "rt-invitees-1".to_string(),
            summary: "Team Sync".to_string(),
            location: None,
            all_day: false,
            start: chrono::DateTime::parse_from_rfc3339("2026-09-10T09:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            end: chrono::DateTime::parse_from_rfc3339("2026-09-10T09:30:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            travel_time: TravelTime::None,
            repeat: RepeatFrequency::Never,
            repeat_until: None,
            exception_dates: vec![],
            invitees: vec![
                "alice@example.com".to_string(),
                "bob@example.com".to_string(),
            ],
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

        assert_eq!(
            parsed.invitees,
            vec!["alice@example.com".to_string(), "bob@example.com".to_string()]
        );
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

#[cfg(test)]
mod propfind_body_tests {
    use super::*;

    /// RFC 4918 §9.1: a PROPFIND body's root element must be `DAV:propfind`
    /// wrapping `DAV:prop`. SabreDAV (Nextcloud) rejects a bare `DAV:prop`
    /// root with 400 "Expected {DAV:}propfind but received {DAV:}prop";
    /// lenient servers (Apache mod_dav) accept both, which is why the bug
    /// survived until a Nextcloud server was tried.
    #[test]
    fn test_propfind_bodies_are_propfind_wrapped() {
        for body in [
            CalDavClient::PRINCIPAL_PROPFIND,
            CalDavClient::HOME_PROPFIND,
            CalDavClient::LIST_CAL_PROPFIND,
        ] {
            let doc = roxmltree::Document::parse(body).expect("valid xml");
            let root = doc.root_element();
            assert!(
                root.has_tag_name("propfind"),
                "root element must be DAV:propfind, got {:?}",
                root.tag_name()
            );
            assert!(
                root.descendants().any(|e| e.has_tag_name("prop")),
                "propfind must wrap a DAV:prop element"
            );
        }
    }
}

#[cfg(test)]
mod propstat_status_tests {
    use super::*;

    /// When the server answers a property with a non-2xx propstat status
    /// (e.g. `calendar-home-set` → 404 because the CalDAV app is not
    /// enabled), the error must say so — not the generic "no X href"
    /// parse error that hid this from the user.
    #[test]
    fn test_propstat_404_reports_status() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/remote.php/dav/principals/users/jlagman/</d:href>
    <d:propstat>
      <d:prop><d:calendar-home-set/></d:prop>
      <d:status>HTTP/1.1 404 Not Found</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let err = propstat_href(&doc, "calendar-home-set").unwrap_err();
        assert!(
            err.to_string().contains("404"),
            "error should carry the server status, got: {err}"
        );
        assert!(
            err.to_string().contains("calendar-home-set"),
            "error should name the property, got: {err}"
        );
    }

    /// A 2xx propstat with an href child resolves normally.
    #[test]
    fn test_propstat_200_resolves_href() {
        let xml = r#"<?xml version="1.0"?>
<d:multistatus xmlns:d="DAV:">
  <d:response>
    <d:href>/remote.php/dav/principals/users/user/</d:href>
    <d:propstat>
      <d:prop>
        <d:calendar-home-set>
          <d:href>/remote.php/dav/calendars/user/</d:href>
        </d:calendar-home-set>
      </d:prop>
      <d:status>HTTP/1.1 200 OK</d:status>
    </d:propstat>
  </d:response>
</d:multistatus>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let href = propstat_href(&doc, "calendar-home-set").unwrap();
        assert_eq!(href, "/remote.php/dav/calendars/user/");
    }
}

#[cfg(test)]
mod user_calendars_fallback_tests {
    use super::*;

    /// The fallback list URL is `{base}/calendars/{username}/` — the
    /// per-user DAV path used by Nextcloud (and the path Thunderbird is
    /// given directly). Base URLs with or without a trailing slash must
    /// produce the same result.
    #[test]
    fn test_alert_minutes_all_variants() {
        assert_eq!(alert_minutes(&AlertTime::None), None);
        assert_eq!(alert_minutes(&AlertTime::AtTime), None);
        assert_eq!(alert_minutes(&AlertTime::FiveMinutes), Some(5));
        assert_eq!(alert_minutes(&AlertTime::TenMinutes), Some(10));
        assert_eq!(alert_minutes(&AlertTime::FifteenMinutes), Some(15));
        assert_eq!(alert_minutes(&AlertTime::ThirtyMinutes), Some(30));
        assert_eq!(alert_minutes(&AlertTime::OneHour), Some(60));
        assert_eq!(alert_minutes(&AlertTime::TwoHours), Some(120));
        assert_eq!(alert_minutes(&AlertTime::OneDay), Some(1440));
        assert_eq!(alert_minutes(&AlertTime::TwoDays), Some(2880));
        assert_eq!(alert_minutes(&AlertTime::OneWeek), Some(10080));
        assert_eq!(alert_minutes(&AlertTime::Custom(45)), Some(45));
        assert_eq!(alert_minutes(&AlertTime::Custom(0)), Some(0));
    }

    #[test]
    fn test_user_calendars_url_trailing_slash_insensitive() {
        assert_eq!(
            CalDavClient::user_calendars_url("https://x/remote.php/dav", "jlagman"),
            "https://x/remote.php/dav/calendars/jlagman/"
        );
        assert_eq!(
            CalDavClient::user_calendars_url("https://x/remote.php/dav/", "jlagman"),
            "https://x/remote.php/dav/calendars/jlagman/"
        );
    }
}
