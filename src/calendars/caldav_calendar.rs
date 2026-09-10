//! CalDAV calendar source.
//!
//! Cache-only by design: the server is the source of truth and all network
//! I/O happens in the update layer via a cloned standalone `CalDavClient`
//! (run in `Task::perform`, since `CalendarManager` is `!Send`). Results are
//! applied to this cache on the UI thread through the `apply_*` methods.

use super::calendar_source::{CalendarInfo, CalendarSource, CalendarType};
use crate::caldav::{CalDavClient, CalendarEvent};
use std::collections::HashMap;
use std::error::Error;

/// A CalDAV-based calendar (Basic-auth servers: Nextcloud, Radicale,
/// self-hosted, Apple via app-password).
#[derive(Debug)]
pub struct CalDavCalendar {
    info: CalendarInfo,
    client: CalDavClient,
    cached_events: Vec<CalendarEvent>,
    /// uid → resource href captured from REPORT responses. New events are
    /// PUT to `{collection_url}/{uid}.ics`; existing events use their href.
    href_map: HashMap<String, String>,
    /// uid → ETag captured from REPORT responses / PUT responses. Sent as
    /// `If-Match` on edits and deletes so a concurrent server-side change
    /// rejects the write (412) instead of clobbering it.
    etag_map: HashMap<String, String>,
}

impl CalDavCalendar {
    /// Create a CalDAV calendar. `collection_url` is the per-calendar
    /// collection URL (absolute).
    pub fn new(
        id: String,
        name: String,
        collection_url: String,
        username: String,
        password: String,
    ) -> Result<Self, Box<dyn Error>> {
        let info = CalendarInfo::new(id, name, CalendarType::CalDav);
        let client = CalDavClient::new(collection_url, username, password)?;

        Ok(CalDavCalendar {
            info,
            client,
            cached_events: Vec::new(),
            href_map: HashMap::new(),
            etag_map: HashMap::new(),
        })
    }

    /// A standalone clone of the client for off-thread network work.
    pub fn client_clone(&self) -> CalDavClient {
        self.client.clone()
    }

    /// The collection URL this calendar REPORTs against.
    pub fn collection_url(&self) -> &str {
        self.client.base_url_public()
    }

    /// The username for this account (for keyring lookups).
    pub fn username(&self) -> &str {
        self.client.username_public()
    }

    /// Get cached events without fetching from server.
    #[allow(dead_code)] // Reserved for future event access (mirrors LocalCalendar::get_events)
    pub fn cached_events(&self) -> &[CalendarEvent] {
        &self.cached_events
    }

    /// Replace the cache with freshly fetched events and their
    /// `(href, etag)` pairs. A `None` etag records the href but no ETag.
    pub fn apply_fetched(
        &mut self,
        events: Vec<CalendarEvent>,
        hrefs: Vec<(String, String, Option<String>)>,
    ) {
        self.href_map.clear();
        self.etag_map.clear();
        for (uid, href, etag) in hrefs {
            self.href_map.insert(uid.clone(), href);
            if let Some(etag) = etag {
                self.etag_map.insert(uid, etag);
            }
        }
        self.cached_events = events;
    }

    /// Record a newly created event and its href + the ETag the server
    /// returned for the PUT.
    pub fn apply_created(&mut self, event: CalendarEvent, href: String, etag: String) {
        self.href_map.insert(event.uid.clone(), href);
        if !etag.is_empty() {
            self.etag_map.insert(event.uid.clone(), etag);
        }
        self.cached_events.push(event);
    }

    /// Record an updated event (href unchanged) and the fresh ETag from the
    /// PUT response.
    pub fn apply_updated(&mut self, event: CalendarEvent, etag: String) {
        if !etag.is_empty() {
            self.etag_map.insert(event.uid.clone(), etag);
        }
        if let Some(existing) = self.cached_events.iter_mut().find(|e| e.uid == event.uid) {
            *existing = event;
        } else {
            self.cached_events.push(event);
        }
    }

    /// Record a deleted event.
    pub fn apply_deleted(&mut self, uid: &str) {
        self.href_map.remove(uid);
        self.etag_map.remove(uid);
        self.cached_events.retain(|e| e.uid != uid);
    }

    /// The href for an existing event, if known.
    pub fn event_href(&self, uid: &str) -> Option<&str> {
        self.href_map.get(uid).map(|s| s.as_str())
    }

    /// The ETag for an existing event, if known. Used as the `If-Match` value
    /// on edits and deletes; `None` means "no ETag captured" and the write
    /// proceeds without `If-Match` (last-writer-wins fallback).
    pub fn event_etag(&self, uid: &str) -> Option<&str> {
        self.etag_map.get(uid).map(|s| s.as_str())
    }

    /// The href a brand-new event with this uid should be PUT to.
    pub fn new_event_href(&self, uid: &str) -> String {
        format!("{}/{}.ics", self.collection_url(), uid)
    }
}

impl CalendarSource for CalDavCalendar {
    fn info(&self) -> &CalendarInfo {
        &self.info
    }

    fn info_mut(&mut self) -> &mut CalendarInfo {
        &mut self.info
    }

    fn fetch_events(&self) -> Result<Vec<CalendarEvent>, Box<dyn Error>> {
        // Cache only — network refresh happens in the update layer
        // (see `apply_fetched`).
        Ok(self.cached_events.clone())
    }

    fn add_event(&mut self, event: CalendarEvent) -> Result<(), Box<dyn Error>> {
        // Cache-only; the network PUT happens in the update layer.
        self.cached_events.push(event);
        Ok(())
    }

    fn update_event(&mut self, event: CalendarEvent) -> Result<(), Box<dyn Error>> {
        // Cache-only; the network PUT happens in the update layer.
        if let Some(existing) = self.cached_events.iter_mut().find(|e| e.uid == event.uid) {
            *existing = event;
        } else {
            self.cached_events.push(event);
        }
        Ok(())
    }

    fn delete_event(&mut self, uid: &str) -> Result<(), Box<dyn Error>> {
        // Cache-only; the network DELETE happens in the update layer.
        self.cached_events.retain(|e| e.uid != uid);
        Ok(())
    }

    fn sync(&mut self) -> Result<(), Box<dyn Error>> {
        // No-op: sync is driven by the update layer (Message::SyncCalendars),
        // which posts results back via apply_fetched.
        Ok(())
    }

    fn remote_config(&self) -> Option<(String, String)> {
        Some((
            self.collection_url().to_string(),
            self.username().to_string(),
        ))
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caldav::{AlertTime, CalendarEvent, RepeatFrequency, TravelTime};

    fn event(uid: &str) -> CalendarEvent {
        CalendarEvent {
            uid: uid.to_string(),
            summary: "Test".to_string(),
            location: None,
            all_day: false,
            start: chrono::Utc::now(),
            end: chrono::Utc::now() + chrono::Duration::hours(1),
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
        }
    }

    fn caldav() -> CalDavCalendar {
        CalDavCalendar::new(
            "cal-1".into(),
            "Work".into(),
            "https://example.com/caldav/work".into(),
            "user".into(),
            "pass".into(),
        )
        .expect("https calendar should build")
    }

    #[test]
    fn apply_fetched_populates_href_and_etag_maps() {
        let mut cal = caldav();
        cal.apply_fetched(
            vec![event("e1"), event("e2")],
            vec![
                (
                    "e1".into(),
                    "https://example.com/caldav/work/e1.ics".into(),
                    Some("\"etag1\"".into()),
                ),
                // e2 has no etag — href is still recorded, etag is not.
                ("e2".into(), "https://example.com/caldav/work/e2.ics".into(), None),
            ],
        );
        assert_eq!(cal.event_href("e1"), Some("https://example.com/caldav/work/e1.ics"));
        assert_eq!(cal.event_etag("e1"), Some("\"etag1\""));
        assert_eq!(cal.event_href("e2"), Some("https://example.com/caldav/work/e2.ics"));
        assert_eq!(cal.event_etag("e2"), None);
    }

    #[test]
    fn event_etag_unknown_uid_is_none() {
        let cal = caldav();
        assert_eq!(cal.event_etag("nope"), None);
    }

    #[test]
    fn apply_created_and_updated_store_etag() {
        let mut cal = caldav();
        cal.apply_created(event("e1"), "https://example.com/caldav/work/e1.ics".into(), "\"a\"".into());
        assert_eq!(cal.event_etag("e1"), Some("\"a\""));

        // A subsequent PUT returns a fresh etag; the map tracks the latest.
        cal.apply_updated(event("e1"), "\"b\"".into());
        assert_eq!(cal.event_etag("e1"), Some("\"b\""));
    }

    #[test]
    fn apply_deleted_clears_both_maps() {
        let mut cal = caldav();
        cal.apply_created(event("e1"), "https://example.com/caldav/work/e1.ics".into(), "\"a\"".into());
        cal.apply_deleted("e1");
        assert_eq!(cal.event_href("e1"), None);
        assert_eq!(cal.event_etag("e1"), None);
    }
}
