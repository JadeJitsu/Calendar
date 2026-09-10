# Calendar - Development TODO

## Recently Completed ✅ (2026-09-10 — v0.3.0)

- [x] **CalDAV write safety (If-Match/ETag)** — edits and deletes send an `If-Match` ETag, so a server-side change since the last sync rejects the write (412) instead of silently clobbering it. ETag captured from the REPORT (was requested but dropped) and from each PUT response, tracked per event beside its href. On a 412: desktop notification + auto re-sync to server truth. New events / pre-first-sync edits fall back to last-writer-wins. Verified live against Nextcloud 34 (stale If-Match → 412). `src/caldav.rs`, `src/calendars/caldav_calendar.rs`, `src/message.rs`, `src/update/caldav.rs`

## Recently Completed ✅ (2026-09-10 — v0.2.0)

- [x] **Week-view input flicker fixed** — hour-grid/chip `on_enter` handlers now attach only during an active drag; idle cursor movement no longer dispatches messages (measured ~260/sweep → 0). `src/views/week/{time_grid,header,events}.rs`
- [x] **Tray/dock restore flicker fixed** — root view paints an opaque background instead of relying on the compositor's blur layer, so recreating the window on restore no longer flashes in tiled workspaces. `src/app.rs` (`view()`)
- [x] **Dock/systray single-instance** — a second launch (dock click, systray, file/URL) forwards to the running instance via a self-contained D-Bus activation service instead of spawning a second process + tray icon. `src/services/single_instance.rs`
- [x] **Build gates green** — `cargo build`, `cargo test` (168/168), `cargo clippy --all-targets`, `cargo build --all-targets` all pass; repo rustfmt'd. Moved 3 ICS round-trip tests to `services/export_handler.rs`; fixed `clippy::eq_op`.
- [x] **Versioning + CHANGELOG** — `Cargo.toml` is source of truth, `CHANGELOG.md`, documented convention in README/CLAUDE.md. First release: **v0.2.0**.

## Recently Completed ✅ (2026-09-09)

- [x] **Event notifications/alerts** — precise one-shot desktop notifications; `src/services/notification_scheduler.rs`
- [x] **Background CalDAV sync** — TimeTick-driven, configurable interval; `src/update/caldav.rs`
- [x] **Settings dialog** — week numbers + background sync interval; `src/components/settings_dialog.rs`
- [x] **Event invites (attendees)** — `ATTENDEE` ICS round-trip on the live write path; `src/services/export_handler.rs`
- [x] **Event search** — live cross-calendar search; `src/services/search.rs` + `src/components/search.rs`
- [x] **CalDAV write path full serializer** — live `event_to_ical` delegates to shared `caldav::build_event`
- [x] **System tray icon + close-to-tray** — StatusNotifierItem on a dedicated GTK thread; `src/services/tray.rs`
- [x] **Dock icon (native build)** — `APP_ID` always `dev.jadejitsu.apps.Calendar` (was `…Devel`)
- [x] **Tray "Show" restore** — close + reopen (Wayland can't un-minimize); `src/update/mod.rs`
- [x] **App-id rebrand** `dev.xarbit` → `dev.jadejitsu` (b8193f5)

## Completed — Event Management & Views ✅

- [x] **Month view** — day cells, quick-event input, "+N more" overflow, calendar selection
- [x] **Week view** — time grid, timed event chips, all-day row, time indicator, drag-to-select time range
- [x] **Day view** — all-day section + time grid; `src/views/day.rs`
- [x] **Year view** — `src/views/year.rs`
- [x] **Event create/edit dialog** — full details (title, location, times, all-day, calendar, recurrence, invitees, reminders); `src/components/event_dialog.rs`
- [x] **Recurring events** — RRULE in/out, exception dates, master-UID occurrence handling
- [x] **All-day + multi-day events** — DTEND survives round-trip
- [x] **Drag to move events** — month + week, with floating preview; `src/selection/drag.rs`
- [x] **Delete event** — confirm dialog
- [x] **Event storage** — SQLite (SQLCipher-ready) at `~/.local/share/sol-calendar/sol.db`; calendar metadata at `~/.config/sol-calendar/calendars.json`
- [x] **CalDAV sync** — RFC 4791 discovery (incl. `calendar-home-set` 404 fallback for Nextcloud 34), push local changes, per-calendar sync/error indicators
- [x] **Import .ics** — file import with calendar selection + progress; `src/update/import.rs`
- [x] **Export .ics** — per-calendar and all; `src/services/export_handler.rs`
- [x] **Subscribe (webcal)** — URL subscription with create-new-calendar option

## Pending 📋

### New calendar backends
- [ ] Google Calendar support
- [ ] iCloud Calendar support

### Technical debt
- [x] **`eprintln!` audit** (2026-09-10) — reviewed: no debt. The only app `eprintln!` are in the debug-only `--dev-reset-db`/`--dev-seed-data` CLI handlers, each already paired with `log::error!`; the rest are in the standalone probe bins (CLI tools, where stderr is correct). Left as-is.
- [x] **CI** — GitHub Actions (`.github/workflows/ci.yml`) gates `main` on `cargo build --all-targets`, `cargo test`, and `cargo clippy --all-targets`. Clippy is gated on exit code (not `-D warnings`) until the pre-existing warnings are cleaned up.

---

## Architecture Notes

### Event Flow
```
User Action → Message → update/ → CalendarManager → CalendarSource → Database (SQLite)
```

### Storage Architecture
```
Calendar Metadata (config)     Events (database)
~/.config/sol-calendar/    →   ~/.local/share/sol-calendar/
├── calendars.json             └── sol.db (SQLite + SQLCipher)
    ├── id, name, color
    └── enabled, type
```

### Key Files
- `src/message.rs` - All messages (grouped by feature domain)
- `src/update/` - Message handlers by domain (navigation, calendar, event, selection, import, caldav)
- `src/components/` - Reusable widgets (event_chip, day_cell, event_dialog, search, settings_dialog)
- `src/views/` - month/ / week/ / day.rs / year.rs / main_view.rs
- `src/caldav.rs` - CalDAV client + ICS serializer (`calendar_event_to_ics`)
- `src/services/export_handler.rs` - ICS import/export + `parse_ical_string`
- `src/services/single_instance.rs` - D-Bus single-instance activation
- `src/services/tray.rs` - StatusNotifierItem (dedicated GTK thread)
- `src/selection/` - Drag selection + event drag state
- `src/calendars/` - Calendar backends (Local, CalDAV) + config
- `src/database/` - SQLite storage

### Data Structures
- `CalendarEvent` - Core event data (uid, summary, start, end, recurrence, invitees, alerts, …)
- `DisplayEvent` - Event with calendar color for rendering
