//! CalDAV message handling: account management, sync, and write-back results.
//!
//! All network I/O runs off the UI thread via `Task::perform` +
//! `spawn_blocking` (the `CalDavClient` is `reqwest::blocking`). Results are
//! applied to the `CalDavCalendar` cache on the UI thread.
//!
//! Security: log only hosts, calendar IDs, UIDs, and status codes — never
//! credentials, event summaries/notes, or full response bodies.

use cosmic::app::Task;
use log::{debug, error, info, warn};

use crate::app::CosmicCalendar;
use crate::calendars::{CalDavCalendar, CalendarSource, CalendarType};
use crate::caldav::{CalDavClient, CalendarEvent, DiscoveredCalendar};
use crate::dialogs::{ActiveDialog, DialogManager};
use crate::message::Message;
use crate::services::{CalDavCredentials, ExportHandler, host_of};

/// Open the add CalDAV account dialog.
pub fn handle_open_add_caldav_dialog(app: &mut CosmicCalendar) {
    DialogManager::close(&mut app.active_dialog);
    app.active_dialog = ActiveDialog::AddCalDav {
        url: String::new(),
        username: String::new(),
        password: String::new(),
    };
}

/// Update the server URL field while typing.
pub fn handle_caldav_dialog_url_changed(app: &mut CosmicCalendar, url: String) {
    if let ActiveDialog::AddCalDav { url: u, .. } = &mut app.active_dialog {
        *u = url;
    }
}

/// Update the username field while typing.
pub fn handle_caldav_dialog_user_changed(app: &mut CosmicCalendar, username: String) {
    if let ActiveDialog::AddCalDav { username: u, .. } = &mut app.active_dialog {
        *u = username;
    }
}

/// Update the password field while typing.
pub fn handle_caldav_dialog_password_changed(app: &mut CosmicCalendar, password: String) {
    if let ActiveDialog::AddCalDav { password: p, .. } = &mut app.active_dialog {
        *p = password;
    }
}

/// Confirm adding a CalDAV account: validate, store credentials in the
/// keyring, then run discovery off-thread.
pub fn handle_confirm_add_caldav(app: &mut CosmicCalendar) -> Task<Message> {
    let (url, username, password) = match &app.active_dialog {
        ActiveDialog::AddCalDav {
            url,
            username,
            password,
        } => (url.trim(), username.trim(), password.clone()),
        _ => return Task::none(),
    };

    // Validate: HTTPS only. Never log the full URL (it may embed
    // credentials); log the host only.
    if !url.starts_with("https://") {
        error!("CalDAV: Rejected non-HTTPS URL");
        return Task::none();
    }
    if username.is_empty() {
        warn!("CalDAV: Empty username, cannot add account");
        return Task::none();
    }

    // Store credentials in the keyring (never in the config file).
    if let Err(e) = CalDavCredentials::store(url, username, &password) {
        error!("CalDAV: Failed to store credentials in keyring: {}", e);
        // Surface the failure instead of silently falling back to
        // plaintext storage.
        app.active_dialog = ActiveDialog::None;
        return Task::none();
    }

    let server_url = url.to_string();
    let user = username.to_string();
    // The Task::perform callback is `Fn` (may be called more than once), so
    // it cannot move these out — it gets its own copies.
    let server_url_cb = server_url.clone();
    let user_cb = user.clone();

    // Discovery off-thread: principal → home-set → calendar list.
    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                let client = match CalDavClient::new(server_url.clone(), user.clone(), password) {
                    Ok(c) => c,
                    Err(e) => {
                        error!("CalDAV: Client construction failed: {}", e);
                        return Err(e.to_string());
                    }
                };
                match client.discover() {
                    Ok(calendars) => Ok(calendars),
                    Err(e) => {
                        // `discover()` returns "no calendars found" when the
                        // home-set is empty — in that case the pasted URL is
                        // itself a collection, so fall back to it.
                        if e.to_string().contains("no calendars found") {
                            Ok(vec![client.single_calendar()])
                        } else {
                            error!("CalDAV: Discovery failed: {}", e);
                            Err(e.to_string())
                        }
                    }
                }
            })
            .await
            .unwrap_or_else(|e| Err(format!("Discovery task panicked: {}", e)))
        },
        move |result| match result {
            Ok(calendars) => cosmic::Action::App(Message::CalDavDiscovered(
                server_url_cb.clone(),
                user_cb.clone(),
                calendars,
            )),
            Err(e) => cosmic::Action::App(Message::CalDavDiscoveryFailed(e)),
        },
    )
}

/// Discovery finished: create a `CalDavCalendar` per discovered calendar,
/// persist config, close the dialog, and trigger a sync.
pub fn handle_caldav_discovered(
    app: &mut CosmicCalendar,
    server_url: String,
    username: String,
    calendars: Vec<DiscoveredCalendar>,
) -> Task<Message> {
    info!(
        "CalDAV: Discovered {} calendar(s) for user '{}'",
        calendars.len(),
        username
    );

    let mut created_any = false;
    for (i, cal) in calendars.iter().enumerate() {
        let id = format!("caldav-{}-{}", host_of(&server_url), i);
        let name = if cal.name.is_empty() {
            format!("Calendar {}", i + 1)
        } else {
            cal.name.clone()
        };

        // The password was stored on confirm; reload it for the client.
        let password = match CalDavCredentials::load(&server_url, &username) {
            Ok(p) => p,
            Err(e) => {
                error!("CalDAV: Keyring load failed after store: {}", e);
                continue;
            }
        };

        match CalDavCalendar::new(
            id.clone(),
            name.clone(),
            cal.url.clone(),
            username.clone(),
            password,
        ) {
            Ok(calendar) => {
                app.calendar_manager.add_source(Box::new(calendar));
                created_any = true;
                debug!("CalDAV: Added calendar '{}' (id={})", name, id);
            }
            Err(e) => {
                error!("CalDAV: Failed to construct calendar '{}': {}", name, e);
            }
        }
    }

    if created_any {
        app.calendar_manager.save_config().ok();
    }

    app.active_dialog = ActiveDialog::None;

    if created_any {
        handle_sync_calendars(app)
    } else {
        Task::none()
    }
}

/// Discovery failed: close the dialog and surface the error.
pub fn handle_caldav_discovery_failed(app: &mut CosmicCalendar, error_message: String) {
    error!("CalDAV: Discovery failed: {}", error_message);
    app.active_dialog = ActiveDialog::None;
    // TODO(Phase 6): show a toast/dialog with the error message.
}

/// Sync all enabled CalDAV calendars. Each calendar is fetched off-thread
/// and its result posted back as `CalDavSynced`/`CalDavSyncFailed`.
pub fn handle_sync_calendars(app: &mut CosmicCalendar) -> Task<Message> {
    // Collect (id, collection_url, username, password) for each enabled
    // CalDAV source. Passwords come from the keyring.
    let mut jobs = Vec::new();
    for source in app.calendar_manager.sources_mut() {
        if source.info().calendar_type != CalendarType::CalDav || !source.is_enabled() {
            continue;
        }
        let Some(caldav) = source.as_any().downcast_ref::<CalDavCalendar>() else {
            continue;
        };
        // Pull everything out of the downcasted source before any other
        // borrow of `source` — `as_any()` holds a mutable borrow.
        let id = caldav.info().id.clone();
        let collection_url = caldav.collection_url().to_string();
        let username = caldav.username().to_string();

        let password = match CalDavCredentials::load(&collection_url, &username) {
            Ok(p) => p,
            Err(e) => {
                error!("CalDAV: Keyring load failed for '{}': {}", id, e);
                jobs.push((
                    id,
                    collection_url,
                    username,
                    Err(format!("Keyring unavailable: {}", e)),
                ));
                continue;
            }
        };

        jobs.push((id, collection_url, username, Ok(password)));
    }

    if jobs.is_empty() {
        debug!("CalDAV: No enabled CalDAV calendars to sync");
        return Task::none();
    }

    info!("CalDAV: Syncing {} calendar(s)", jobs.len());

    // Mark the first as syncing for the UI spinner.
    app.sync_status = jobs.first().map(|(id, ..)| (id.clone(), true));

    // One `Task::perform` per calendar; each posts its own result message.
    let tasks: Vec<Task<Message>> = jobs
        .into_iter()
        .map(|(id, collection_url, username, password)| {
            let id_future = id.clone();
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        password.and_then(|password| {
                            let client = CalDavClient::new(collection_url.clone(), username, password)
                                .map_err(|e| e.to_string())?;
                            let pairs = client
                                .fetch_events(&collection_url)
                                .map_err(|e| e.to_string())?;
                            let mut events = Vec::new();
                            let mut hrefs = Vec::new();
                            for (href, ics) in pairs {
                                match ExportHandler::parse_ical_string(&ics) {
                                    Ok(mut parsed) => {
                                        for event in parsed.drain(..) {
                                            hrefs.push((event.uid.clone(), href.clone()));
                                            events.push(event);
                                        }
                                    }
                                    Err(e) => {
                                        warn!(
                                            "CalDAV: Skipping unparseable event in '{}': {}",
                                            id_future, e
                                        );
                                    }
                                }
                            }
                            Ok((events, hrefs))
                        })
                    })
                    .await
                    .unwrap_or_else(|e| Err(format!("Sync task panicked: {}", e)))
                },
                move |result| match result {
                    Ok((events, hrefs)) => {
                        let count = events.len();
                        debug!("CalDAV: Synced {} events for '{}'", count, id);
                        cosmic::Action::App(Message::CalDavSynced(id.clone(), events, hrefs))
                    }
                    Err(e) => {
                        error!("CalDAV: Sync failed for '{}': {}", id, e);
                        cosmic::Action::App(Message::CalDavSyncFailed(id.clone(), e.to_string()))
                    }
                },
            )
        })
        .collect();

    Task::batch(tasks)
}

/// A calendar sync started (UI spinner).
pub fn handle_caldav_sync_started(app: &mut CosmicCalendar, calendar_id: String) {
    app.sync_status = Some((calendar_id, true));
}

/// A calendar sync finished: apply fetched events to the cache.
pub fn handle_caldav_synced(
    app: &mut CosmicCalendar,
    calendar_id: String,
    events: Vec<CalendarEvent>,
    hrefs: Vec<(String, String)>,
) {
    info!("CalDAV: Sync complete for '{}' ({} events)", calendar_id, events.len());
    if let Some(source) = app
        .calendar_manager
        .sources_mut()
        .iter_mut()
        .find(|s| s.info().id == calendar_id)
    {
        if let Some(caldav) = source.as_any().downcast_mut::<CalDavCalendar>() {
            caldav.apply_fetched(events, hrefs);
        }
    }
    app.sync_status = None;
    app.refresh_cached_events();
}

/// A calendar sync failed.
pub fn handle_caldav_sync_failed(
    app: &mut CosmicCalendar,
    calendar_id: String,
    error_message: String,
) {
    error!("CalDAV: Sync failed for '{}': {}", calendar_id, error_message);
    // Persist the error so the sidebar can show an error tint on this
    // calendar. The bool is `is_syncing` — `false` here means "errored",
    // distinct from `None` (idle/cleared).
    app.sync_status = Some((calendar_id, false));
}

/// An event was created on the server: apply to the cache.
pub fn handle_caldav_event_created(
    app: &mut CosmicCalendar,
    calendar_id: String,
    event: CalendarEvent,
    href: String,
) {
    if let Some(source) = app
        .calendar_manager
        .sources_mut()
        .iter_mut()
        .find(|s| s.info().id == calendar_id)
    {
        if let Some(caldav) = source.as_any().downcast_mut::<CalDavCalendar>() {
            caldav.apply_created(event, href);
        }
    }
    app.refresh_cached_events();
}

/// An event was updated on the server: apply to the cache.
pub fn handle_caldav_event_updated(
    app: &mut CosmicCalendar,
    calendar_id: String,
    event: CalendarEvent,
    _href: String,
) {
    if let Some(source) = app
        .calendar_manager
        .sources_mut()
        .iter_mut()
        .find(|s| s.info().id == calendar_id)
    {
        if let Some(caldav) = source.as_any().downcast_mut::<CalDavCalendar>() {
            caldav.apply_updated(event);
        }
    }
    app.refresh_cached_events();
}

/// An event was deleted on the server: apply to the cache.
pub fn handle_caldav_event_deleted(app: &mut CosmicCalendar, calendar_id: String, uid: String) {
    if let Some(source) = app
        .calendar_manager
        .sources_mut()
        .iter_mut()
        .find(|s| s.info().id == calendar_id)
    {
        if let Some(caldav) = source.as_any().downcast_mut::<CalDavCalendar>() {
            caldav.apply_deleted(&uid);
        }
    }
    app.refresh_cached_events();
}

/// A CalDAV write operation failed.
pub fn handle_caldav_write_failed(
    _app: &mut CosmicCalendar,
    calendar_id: String,
    error_message: String,
) {
    error!("CalDAV: Write failed for '{}': {}", calendar_id, error_message);
    // No toast/notification widget exists in this app yet — the error is
    // logged. The sync-status indicator covers sync failures; write failures
    // surface via the log for now.
}

/// PUT an event to a CalDAV calendar off-thread. Returns a `Task` that posts
/// `CalDavEventCreated`/`CalDavEventUpdated` on success or `CalDavWriteFailed`
/// on error. `is_edit` selects the target href: new events go to
/// `{collection}/{uid}.ics`; edits use the event's known href, falling back to
/// the new-event path when the href map has no entry yet.
///
/// v1 is last-writer-wins: a plain full-replace PUT with no `If-Match`. The
/// returned ETag is logged (not stored) for a future concurrency phase.
pub fn write_caldav_event(
    app: &mut CosmicCalendar,
    calendar_id: String,
    event: CalendarEvent,
    is_edit: bool,
) -> Task<Message> {
    let Some(source) = app
        .calendar_manager
        .sources_mut()
        .iter_mut()
        .find(|s| s.info().id == calendar_id)
    else {
        error!("CalDAV: Write target calendar '{}' not found", calendar_id);
        return Task::none();
    };
    let Some(caldav) = source.as_any().downcast_mut::<CalDavCalendar>() else {
        // Not a CalDAV calendar — the caller should not have routed here.
        return Task::none();
    };

    let uid = event.uid.clone();
    let href = if is_edit {
        caldav
            .event_href(&uid)
            .map(|s| s.to_string())
            .unwrap_or_else(|| caldav.new_event_href(&uid))
    } else {
        caldav.new_event_href(&uid)
    };
    let client = caldav.client_clone();
    let ics = ExportHandler::event_to_ical(&event).to_string();
    // The callback is `Fn` + `'static`, so it needs its own copies of the
    // values the async block moves.
    let href_cb = href.clone();

    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                client
                    .put_event(&href, &ics, None)
                    .map_err(|e| e.to_string())
            })
            .await
            .unwrap_or_else(|e| Err(format!("Write task panicked: {}", e)))
        },
        move |result| match result {
            Ok(etag) => {
                debug!(
                    "CalDAV: Wrote event '{}' to '{}' (etag={:?})",
                    uid, calendar_id, etag
                );
                if is_edit {
                    cosmic::Action::App(Message::CalDavEventUpdated(
                        calendar_id.clone(),
                        event.clone(),
                        href_cb.clone(),
                    ))
                } else {
                    cosmic::Action::App(Message::CalDavEventCreated(
                        calendar_id.clone(),
                        event.clone(),
                        href_cb.clone(),
                    ))
                }
            }
            Err(e) => {
                error!("CalDAV: Write failed for '{}' ({}): {}", calendar_id, uid, e);
                cosmic::Action::App(Message::CalDavWriteFailed(calendar_id.clone(), e))
            }
        },
    )
}

/// DELETE an event from a CalDAV calendar off-thread. Returns a `Task` that
/// posts `CalDavEventDeleted` on success or `CalDavWriteFailed` on error.
pub fn delete_caldav_event(
    app: &mut CosmicCalendar,
    calendar_id: String,
    uid: String,
) -> Task<Message> {
    let Some(source) = app
        .calendar_manager
        .sources_mut()
        .iter_mut()
        .find(|s| s.info().id == calendar_id)
    else {
        error!("CalDAV: Delete target calendar '{}' not found", calendar_id);
        return Task::none();
    };
    let Some(caldav) = source.as_any().downcast_mut::<CalDavCalendar>() else {
        return Task::none();
    };

    // Address the resource by its known href; fall back to the conventional
    // `{collection}/{uid}.ics` path when the href map has no entry.
    let href = caldav
        .event_href(&uid)
        .map(|s| s.to_string())
        .unwrap_or_else(|| caldav.new_event_href(&uid));
    let client = caldav.client_clone();

    Task::perform(
        async move {
            tokio::task::spawn_blocking(move || {
                client
                    .delete_event(&href, None)
                    .map_err(|e| e.to_string())
            })
            .await
            .unwrap_or_else(|e| Err(format!("Delete task panicked: {}", e)))
        },
        move |result| match result {
            Ok(()) => {
                debug!("CalDAV: Deleted event '{}' from '{}'", uid, calendar_id);
                cosmic::Action::App(Message::CalDavEventDeleted(
                    calendar_id.clone(),
                    uid.clone(),
                ))
            }
            Err(e) => {
                error!("CalDAV: Delete failed for '{}' ({}): {}", calendar_id, uid, e);
                cosmic::Action::App(Message::CalDavWriteFailed(calendar_id.clone(), e))
            }
        },
    )
}
