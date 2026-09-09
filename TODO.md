# Calendar - Development TODO

## Recently Completed ✅ (2026-09-09)

- [x] **Event notifications/alerts** — precise one-shot desktop notifications (fires at the exact due instant); `src/services/notification_scheduler.rs`
- [x] **Background CalDAV sync** — TimeTick-driven, configurable interval (Settings); `src/update/caldav.rs`
- [x] **Settings dialog** — week numbers + background sync interval; `src/components/settings_dialog.rs`
- [x] **Event invites (attendees)** — `ATTENDEE` ICS round-trip on the live CalDAV write path; `src/services/export_handler.rs`
- [x] **Event search** — live cross-calendar search (summary/location/notes/invitees); `src/services/search.rs` + `src/components/search.rs`
- [x] **CalDAV write path full serializer** — live `event_to_ical` now delegates to shared `caldav::build_event`, so recurrence/reminders/all-day/attendees survive a PUT
- [x] **System tray icon + close-to-tray** — always-visible StatusNotifierItem (Show/Quit) on a dedicated GTK thread (libappindicator needs a running GTK loop); `close_to_tray` setting minimizes the window to the tray on close instead of exiting; `src/services/tray.rs`
- [x] **Dock icon (native build)** — fixed the app-id mismatch: the non-Flatpak build advertised `…Calendar.Devel`, which broke both the window icon lookup and the dock's window→launcher match; `APP_ID` is now always `dev.xarbit.apps.Calendar`

## Current Sprint: Event Management

### Completed ✅

- [x] **Calendar Selection UI** - Click on calendar name in sidebar to select as active calendar for new events
  - Added `selected_calendar_id` to app state
  - Calendar list shows selection with accent color (Suggested button style)
  - `SelectCalendar(String)` message implemented

- [x] **Event Messages** - Core message types for event operations
  - `StartQuickEvent(NaiveDate)` - Start creating quick event on date
  - `QuickEventTextChanged(String)` - Update quick event text
  - `CommitQuickEvent` - Save the quick event
  - `CancelQuickEvent` - Cancel quick event editing
  - `DeleteEvent(String)` - Delete event by UID

- [x] **Quick Event Input Component** - Inline text field for fast event creation
  - `render_quick_event_input()` - Styled input with calendar color
  - Supports Enter to commit, Escape to cancel (message-based)

- [x] **Event Chip Component** - Display event in day cell
  - `render_event_chip()` - Compact event display with calendar color
  - `render_events_column()` - Stack of events with "+N more" overflow

- [x] **Day Cell with Events** - Enhanced day cell for month view
  - `DayCellConfig` struct with events and quick_event fields
  - `render_day_cell_with_events()` - Full day cell with event support
  - Double-click to start quick event creation

- [x] **Event Storage Backend** - Persist events to SQLite database
  - `handle_commit_quick_event()` creates CalendarEvent with UUID
  - Events stored via LocalCalendar → Database → SQLite
  - Path: `~/.local/share/sol-calendar/sol.db`

- [x] **Month View Events Structure** - Data passing for events
  - `MonthViewEvents` struct with events_by_day HashMap
  - Quick event state tracking (date, text, color)

### In Progress 🔄

- [ ] **Testing Event Creation** - Verify events are created and persisted correctly
  - Double-click on day to show quick event input
  - Type event name and press Enter to save
  - Check events persist across app restarts

### Completed ✅ (Phase 1)

- [x] **Wire Up Event Display** - Show events in month view
  - Added `get_display_events_for_month()` to CalendarManager
  - Caching events in app state (`cached_month_events`)
  - Events refresh on navigation and after add/delete
  - Lifetime issues resolved by owning data in app state

- [x] **Basic Event Display**
  - Display events in month view day cells
  - Show quick event input when double-clicking a day
  - Color events based on their calendar's color

### Pending 📋

#### Phase 2: Event Interaction
- [ ] Click on event chip to select/edit
- [ ] Delete event (context menu or keyboard)
- [ ] Edit event title inline
- [ ] Move event between days (drag & drop)

#### Phase 3: Full Event Dialog
- [ ] Event creation dialog with full details
  - Title, description, location
  - Start/end date and time
  - All-day toggle
  - Calendar selection dropdown
- [ ] Event editing dialog
- [ ] Recurring events support

#### Phase 4: Week/Day View Events
- [ ] Display events in week view time grid
- [ ] Display events in day view time grid
- [ ] Time-based event positioning
- [ ] Multi-day event spanning

#### Phase 5: CalDAV Integration
- [x] CalDAV sync implementation
  - [x] Parse iCalendar data from server
  - [x] Push local changes to server
  - [x] Nextcloud Calendar support (incl. `calendar-home-set` 404 fallback)
- [ ] Google Calendar support
- [ ] iCloud Calendar support

#### Phase 6: Import/Export
- [ ] Import iCal file (.ics)
- [ ] Export calendar to iCal
- [ ] Bulk import/export

### Technical Debt 🔧

- [ ] Clean up unused imports (cargo fix suggestions)
- [ ] Remove dead code warnings
- [ ] Better error handling (replace eprintln with proper logging)
- [ ] Add unit tests for event operations
- [x] ~~Consider caching events in app state for better lifetime management~~ (done)
- [x] **Migrated event storage to SQLite with SQLCipher**
  - Calendar metadata (name, color, enabled) stored in config file: `~/.config/sol-calendar/calendars.json`
  - Events stored in SQLite database: `~/.local/share/sol-calendar/sol.db`
  - Better separation of concerns (config vs data)
  - Encryption support via SQLCipher for event data (ready to use)
  - Efficient indexed queries for date ranges

---

## Architecture Notes

### Event Flow
```
User Action → Message → update.rs → CalendarManager → CalendarSource → Database (SQLite)
```

### Storage Architecture
```
Calendar Metadata (config)     Events (database)
~/.config/sol-calendar/    →   ~/.local/share/sol-calendar/
├── calendars.json             └── sol.db (SQLite + SQLCipher)
    ├── id, name, color
    ├── enabled, type
```

### Key Files
- `src/message.rs` - Event-related messages
- `src/update.rs` - Message handlers including `handle_commit_quick_event()`
- `src/components/event_chip.rs` - Event display components
- `src/components/day_cell.rs` - Day cell with event support
- `src/views/month.rs` - Month view with `MonthViewEvents`
- `src/calendars/` - Calendar backend (LocalCalendar, CalDAV)
- `src/calendars/config.rs` - Calendar metadata (JSON config)
- `src/database/` - SQLite database for events (with SQLCipher encryption support)

### Data Structures
- `CalendarEvent` - Core event data (uid, summary, start, end, etc.)
- `DisplayEvent` - Event with calendar color for rendering
- `DayCellConfig` - Configuration for rendering day cells
- `MonthViewEvents` - Events grouped by day for month view
