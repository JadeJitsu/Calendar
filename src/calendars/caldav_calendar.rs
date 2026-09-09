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
    pub fn cached_events(&self) -> &[CalendarEvent] {
        &self.cached_events
    }

    /// Replace the cache with freshly fetched events and their hrefs.
    pub fn apply_fetched(&mut self, events: Vec<CalendarEvent>, hrefs: Vec<(String, String)>) {
        self.href_map = hrefs.into_iter().collect();
        self.cached_events = events;
    }

    /// Record a newly created event and its href.
    pub fn apply_created(&mut self, event: CalendarEvent, href: String) {
        self.href_map.insert(event.uid.clone(), href);
        self.cached_events.push(event);
    }

    /// Record an updated event (href unchanged).
    pub fn apply_updated(&mut self, event: CalendarEvent) {
        if let Some(existing) = self.cached_events.iter_mut().find(|e| e.uid == event.uid) {
            *existing = event;
        } else {
            self.cached_events.push(event);
        }
    }

    /// Record a deleted event.
    pub fn apply_deleted(&mut self, uid: &str) {
        self.href_map.remove(uid);
        self.cached_events.retain(|e| e.uid != uid);
    }

    /// The href for an existing event, if known.
    pub fn event_href(&self, uid: &str) -> Option<&str> {
        self.href_map.get(uid).map(|s| s.as_str())
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
        Some((self.collection_url().to_string(), self.username().to_string()))
    }

    fn as_any(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
