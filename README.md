<div align="center">    
  <img src="res/icons/hicolor/scalable/apps/dev.xarbit.apps.Calendar.svg" alt="Calendar Icon" width="300" height="300">

  # A Calendar made for the COSMIC Desktop
</div>

![Month View](res/screenshots/default.png)

![Week view](res/screenshots/weekview.png)

![Day view](res/screenshots/day.png)

![Year view](res/screenshots/year.png)

> ⚠️ **WORK IN PROGRESS** - This project is in early development and is **not ready for production use**. Many features are incomplete or missing. Use at your own risk!

## Fork

This project is a **fork of [xarbit/sol](https://github.com/xarbit/sol)** — "The missing native Calendar app for the COSMIC Desktop" by [xarbit](https://github.com/xarbit) (app ID `dev.xarbit.apps.Calendar`, licensed under GPLv3). All credit for the original design, UI foundation, and codebase goes to the upstream project. This fork builds on that work to add full CalDAV support (read + write) and related calendar functionality.

A modern calendar application built with [libcosmic](https://github.com/pop-os/libcosmic), featuring CalDAV support for seamless calendar synchronization.

## About

Calendar is a native calendar application designed for the COSMIC desktop environment. Built using libcosmic's widget system, it provides a clean, intuitive interface inspired by other popular calendar applications while following COSMIC's design language and responsive layout patterns.

The application supports the CalDAV protocol (RFC 4791) for synchronizing events with calendar servers like Nextcloud, Radicale, and other standard CalDAV-compatible services.

## Current Status

This project is in **active development**. Core calendar functionality — event CRUD, CalDAV read/write, reminders, background sync, search, and attendees — is implemented; remaining work is listed under "Work In Progress" below.

### ✅ Implemented Features

#### Core UI
- Mini calendar in sidebar for quick date navigation
- Day selection with visual feedback (outlined today, filled selection)
- Square day cells with 4px rounded corners (theme-independent)
- Instant responsive UI that adapts to window size
- Sidebar overlay mode for small screens (COSMIC Files-style)
- COSMIC-style menu bar (File, Edit, View)
- Navigation controls (Previous/Next/Today buttons)

#### Multiple View Modes
- **Month View**: Full month calendar grid with week numbers (optional)
  - Quick event creation by clicking on day cells
  - Multi-day event selection via drag
  - Event chips with color coding
- **Week View**: Week schedule with hourly time slots
  - Side-by-side layout for overlapping events
  - Current time indicator (red line spanning all days, dot on today)
  - Auto-scroll to current time when entering view
  - Time-slot drag selection for creating timed events
  - All-day events section at the top
  - Drag-and-drop event rescheduling
- **Day View**: Single day detailed schedule with hourly breakdown
- **Year View**: 12-month overview in 3×4 grid

#### Event Management
- Quick event creation via click or keyboard
- Timed event creation with drag selection in week view
- Event editing dialog with full details
- Drag-and-drop event rescheduling (month and week views)
- Event deletion
- Event invitees (attendees) — round-tripped through CalDAV as `ATTENDEE`
- SQLite database persistence

#### Notifications & Background Sync
- Event reminders/alerts with precise one-shot desktop notifications (fires at the exact due instant, not on a polling tick)
- Background CalDAV sync on a configurable interval (Settings → sync interval: 5/15/30 min or 1 hour)
- Settings dialog (week numbers, background sync interval)

#### Search
- Live event search (header search button) across all enabled calendars
- Case-insensitive match on summary, location, notes, and invitee emails
- Click a result to open that event for editing

#### Navigation & Controls
- View switcher buttons (Day/Week/Month/Year) in toolbar
- Keyboard shortcuts for quick navigation:
  - `Ctrl+1` - Month View
  - `Ctrl+2` - Week View
  - `Ctrl+3` - Day View
  - `Ctrl+4` - Year View
  - `Ctrl+N` - New Event
  - `T` - Jump to Today
  - `Left/Right` - Navigate previous/next period

#### Localization
- System locale detection with fallback to English
- Localized month and day names
- First day of week respects locale (Monday/Sunday)
- Week number calculation (ISO 8601)
- 16 languages supported (cs, da, de, el, en, es, fi, fr, it, nl, no, pl, pt, ro, sv, uk)

#### Calendar Management
- Multiple calendar support with color coding
- Calendar visibility toggle
- Custom color picker for calendars
- Create, edit, and delete calendars
- Default calendars: Personal (blue), Work (purple)

#### CalDAV (RFC 4791)
- Add-account dialog (HTTPS-only, Basic auth; password stored in the system keyring, never in the config file)
- RFC 4791 discovery: `current-user-principal` → `calendar-home-set` → calendar list
- **Discovery fallback**: when the server doesn't expose `calendar-home-set` (observed on Nextcloud 34 — the property 404s even though the collections exist), discovery falls back to listing `{base}/calendars/{uid}/` directly, the path direct-URL clients like Thunderbird use
- Full event sync on startup and on demand (menu → Sync), with per-calendar sync/error indicators in the sidebar
- Write-back: create/edit/delete events via CalDAV PUT/DELETE (last-writer-wins, no `If-Match` yet)
- Recurring events (RRULE in/out), all-day events, TZID-aware datetime conversion
- Attendees: `ATTENDEE` properties are written on create/edit and read back on sync

### 🚧 Work In Progress

- [ ] CalDAV write path for recurrence/reminders/all-day (the live `event_to_ical` PUT path currently emits summary/location/notes/url/attendees but not RRULE/EXDATE/VALARM/DTSTAMP — a fuller serializer exists in `calendar_event_to_ics` and is the refactor target)
- [ ] Google Calendar support
- [ ] iCloud Calendar support

## Building

### Prerequisites

- Rust (latest stable version)
- libcosmic dependencies (automatically fetched from git)

### Compile

```bash
cargo build --release
```

### Run

```bash
cargo run --release
```

## Architecture

Calendar follows the **Elm/MVU (Model-View-Update)** architecture pattern, which is standard for libcosmic applications:

### Module Organization

```
src/
├── app.rs                  # Main application state and COSMIC framework integration
├── main.rs                 # Entry point
├── message.rs              # Application message enum
├── keyboard.rs             # Centralized keyboard shortcuts
├── layout.rs               # Responsive layout management
├── selection.rs            # Drag selection and event drag state
│
├── update/                 # Message handling (split by domain)
│   ├── mod.rs              # Main message dispatcher (+ background sync, search)
│   ├── navigation.rs       # View navigation handlers
│   ├── calendar.rs         # Calendar management handlers
│   ├── event.rs            # Event CRUD handlers
│   ├── caldav.rs           # CalDAV sync + event write-back handlers
│   └── selection/          # Selection and drag handlers
│
├── models/                 # Domain models and state
│   ├── calendar_state.rs   # Month calendar state with caching
│   ├── week_state.rs       # Week view state
│   ├── day_state.rs        # Day view state
│   └── year_state.rs       # Year view state
│
├── views/                  # View rendering functions (pure functions)
│   ├── main_view.rs        # Main content coordinator
│   ├── month.rs            # Month grid view
│   ├── week.rs             # Week schedule view with time indicator
│   ├── day.rs              # Day schedule view
│   ├── year.rs             # Year overview
│   └── sidebar.rs          # Sidebar layout
│
├── components/             # Reusable UI components
│   ├── day_cell.rs         # Individual day cell with events
│   ├── mini_calendar.rs    # Mini calendar widget
│   ├── calendar_list.rs    # Calendar list widget
│   ├── color_picker.rs     # Color selection widget
│   ├── toolbar.rs          # Navigation toolbar
│   ├── time_grid.rs        # Hour-based time grid for week/day views
│   ├── event_chip.rs       # Event display chips
│   ├── header_menu.rs      # Application menu bar
│   ├── search.rs           # Search bar (input + results list)
│   └── settings_dialog.rs  # Settings dialog (week numbers, sync interval)
│
├── dialogs/                # Dialog management
│   ├── mod.rs              # Dialog types and state
│   └── manager.rs          # Dialog lifecycle management
│
├── services/               # Business logic services
│   ├── calendar_handler.rs # Calendar CRUD operations
│   ├── event_handler.rs    # Event CRUD operations
│   ├── settings_handler.rs # Settings persistence
│   ├── search.rs           # Pure event search (query → results)
│   ├── export_handler.rs   # iCalendar import/export + ICS round-trip
│   └── notification_scheduler.rs # Due-window + precise alert timing
│
├── database/               # Data persistence
│   └── schema.rs           # SQLite schema and queries
│
├── calendars/              # Calendar data sources
│   ├── calendar_source.rs  # Calendar trait definition
│   ├── local_calendar.rs   # Local calendar implementation
│   └── caldav_calendar.rs  # CalDAV calendar implementation
│
├── locale.rs               # Locale detection and formatting
├── localized_names.rs      # Localized month/day names
├── cache.rs                # Calendar state caching
├── caldav.rs               # CalDAV protocol types
├── settings.rs             # Persistent app settings
├── ui_constants.rs         # UI dimensions, spacing, and colors
└── styles.rs               # Custom styles for containers
```

### Key Architecture Patterns

- **MVU Pattern**: Clean separation of Model (state), View (rendering), and Update (state transitions)
- **Pure View Functions**: All views are pure functions that take state and return UI elements
- **Centralized State**: Single source of truth in `CosmicCalendar` struct
- **Message-Based Updates**: All state changes happen through message passing
- **Caching Layer**: `CalendarCache` pre-computes calendar states for performance
- **Calendar Abstraction**: `CalendarSource` trait enables pluggable calendar backends

## Technology Stack

- **[libcosmic](https://github.com/pop-os/libcosmic)**: Modern UI framework for COSMIC desktop built on iced
- **chrono**: Date and time handling with timezone support
- **chrono-tz**: Timezone database
- **i18n-embed**: Internationalization framework
- **fluent**: Localization system (Mozilla Fluent)
- **icalendar**: iCalendar format parsing and generation
- **reqwest**: HTTP client for CalDAV operations
- **serde**: Serialization/deserialization
- **dirs**: Platform-specific directory handling
- **ron**: Rusty Object Notation for settings storage

## CalDAV Implementation Notes

### Protocol requirements that bite

- **PROPFIND body root must be `DAV:propfind`** (RFC 4918 §9.1), wrapping
  `DAV:prop`. SabreDAV (Nextcloud) rejects a bare `DAV:prop` root with
  `400 "Expected {DAV:}propfind but received {DAV:}prop"`; lenient servers
  (Apache mod_dav) accept both, so a bare root can pass against one server
  and 400 against another. The bodies live in the `*_PROPFIND` constants in
  `src/caldav.rs`, with a regression test asserting the root element.
- **A 207 Multi-Status can still mean "property not found"** — each
  `d:propstat` carries its own `d:status`. Nextcloud answers a missing
  `calendar-home-set` with `404 Not Found` and an empty property element.
  `propstat_href()` reports the server's status instead of a generic
  "no X href" parse error.
- **Nextcloud checks auth before routing**: a `401` does *not* prove an
  endpoint exists. A missing OCS route answers `404 "Invalid query"` only
  *after* auth succeeds.

### Server quirk: missing `calendar-home-set`

Observed on a Nextcloud 34.0.3 host: the Calendar app is enabled and the
web UI works, but PROPFIND for `calendar-home-set` on the user principal
returns a per-property 404, and the DAV root doesn't advertise
`calendar-user-set`. The per-user collections nonetheless exist and list
fine at `{base}/calendars/{uid}/` — the path direct-URL clients
(Thunderbird) use. `CalDavClient::discover()` catches the home-set failure
and falls back to listing that path (uid taken from the principal URL's
last path component).

### Debug probes

`src/bin/` contains standalone probes that drive the real client code
against a live server (print only URLs, counts, UIDs, and status codes —
never credentials or event bodies):

- `caldav_probe <url> <user> <pass>` — full `discover()` chain
- `list_probe <url> <user> <pass>` — PROPFIND a URL and list its collections
- `report_probe <url> <user> <pass>` — REPORT a collection, list event UIDs

Note: Nextcloud's bruteforce protection returns `429` after rapid
repeated requests — space out probe runs.

## Design Philosophy

- **Native COSMIC integration**: Uses libcosmic for native look and feel
- **Responsive design**: Adapts to different window sizes with instant transitions
- **Theme-independent styling**: Critical UI elements maintain consistent appearance
- **Offline-first**: Local storage with server sync
- **Privacy-focused**: Events stored locally by default

## Recent Improvements

### Week View Polish (Latest)
- **Current time indicator**: Red line spanning all days with dot on today's column
- **Auto-scroll**: Week view automatically scrolls to current time on entry
- **Timer updates**: Time indicator updates every 30 seconds
- **Overlapping events**: Side-by-side layout for concurrent events
- **Time-slot selection**: Drag to select time range for new timed events
- **Event drag-and-drop**: Reschedule events by dragging to new dates

### Event Management
- **Quick event creation**: Click day cell or use keyboard shortcut
- **Event dialog**: Full-featured editor with title, location, times, calendar selection
- **Database persistence**: SQLite storage with automatic schema creation
- **Event chips**: Color-coded display in month and week views

### Architecture Improvements
- **Modular update handlers**: Split `update.rs` into domain-specific modules
- **Dialog management**: Centralized dialog state in `dialogs/manager.rs`
- **Service layer**: Business logic separated into `services/` directory
- **Selection state**: Unified drag selection for dates and time slots

## Contributing

Contributions are welcome! However, please note that this project is in early development and the architecture may change significantly. Feel free to:

- Report bugs and issues
- Suggest features
- Submit pull requests
- Improve documentation

## License

This project is licensed under the GNU General Public License v3.0 (GPLv3). See the [LICENSE](LICENSE) file for details.

## Disclaimer

**This software is NOT ready for production use.** Features are incomplete, bugs are expected, and data loss may occur. Do not rely on this application for important calendar events at this time.
