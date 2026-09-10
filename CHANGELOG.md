# Changelog

All notable changes to the Calendar app are documented here.

**Versioning convention** (from v0.2.0 onward):
- `Cargo.toml` `version` is the single source of truth; the Flatpak manifest
  (`dev.jadejitsu.apps.Calendar.yml`) `version` must match it.
- Every user-facing release is a git tag `v<version>` (code) and a GitHub
  Release tagged `v<version>-flatpak` carrying the `.flatpak` bundle.
- Bump `patch` for fixes, `minor` for new features, `major` for breaking
  changes. Update this file in the same commit as the version bump.

## [0.3.0] — 2026-09-10

### Added
- **CalDAV write safety (optimistic concurrency).** CalDAV edits and deletes
  now send an `If-Match` header carrying the event's ETag, so a change made
  on the server since the last sync rejects the write (HTTP 412) instead of
  silently clobbering it. The ETag is captured from the REPORT response (it
  was already requested but previously dropped) and from each PUT's response,
  and tracked per event alongside its href.
  - On a 412 the app shows a desktop notification ("Calendar sync conflict")
    and automatically re-syncs that calendar to server truth, so the view and
    the ETag map catch up; the rejected edit is discarded and can be
    re-applied on the fresh copy.
  - New events, and edits made before the first sync (no ETag captured yet),
    fall back to the previous last-writer-wins behavior.
  - Verified live against a Nextcloud 34 server: a stale `If-Match` returns
    412 (the server enforces it), and the ETag advances on each write.

## [0.2.0] — 2026-09-10

### Fixed
- **Week-view flicker on mouse/keyboard input.** Every hour cell and event
  chip in the week view attached hover handlers unconditionally, so each
  pixel of cursor movement dispatched a message and forced a full view
  rebuild + redraw (measured: ~260 messages per sweep). The `on_enter`
  handlers are now attached only while the corresponding drag is active.
- **Flicker when restoring the window from the tray/dock.** The window is
  transparent with a blurred (acrylic) background; recreating it on restore
  made the compositor re-establish the blur layer, flashing the whole
  window in tiled workspaces. The root view now paints an opaque background.
- **Dock icon / systray single-instance.** A second launch (dock click,
  systray, file/URL open) now forwards to the running instance via a
  self-contained D-Bus activation service instead of spawning a second
  process with a second tray icon. (libcosmic's built-in registration never
  owns the D-Bus name under the tokio backend.)
- **Tray "Show" after close-to-tray** — restores the window correctly
  (close + reopen; Wayland cannot un-minimize).

### Added
- Debug-only flicker tracing (`XCAL_FLICKER_TRACE=<path>`): logs every
  `update()` dispatch and `view()` rebuild to a file. Zero cost in release
  builds. Used to diagnose the flickers above.
- `CHANGELOG.md` and the versioning convention above.

## [0.1.0] — 2026-09-09

First tagged release.

### Highlights
- Full CalDAV read + write (the fork's addition over xarbit/sol)
- Month/week/day views, recurring events, all-day events
- System tray icon + close-to-tray
- Event notifications, background CalDAV sync, event invites, live search
- Flatpak bundle published as a GitHub Release asset
