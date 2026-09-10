//! Self-contained single-instance support (D-Bus activation).
//!
//! Why this exists: libcosmic's `single-instance` feature is broken under the
//! `tokio` backend. Its registration subscription (libcosmic
//! `src/dbus_activation.rs`) only drives its zbus executor under
//! `#[cfg(feature = "smol")]` — under tokio, `request_name` is spawned onto a
//! runtime that never ticks it, so the app-id name is never owned and a second
//! launch never sees a running instance. Rather than fight that, we own the
//! whole mechanism here with the self-contained `zbus::blocking` API:
//!
//! - [`init`] spawns a daemon thread that connects to the session bus, serves
//!   `org.freedesktop.DbusActivation` at `/dev/jadejitsu/apps/Calendar` and
//!   owns the `dev.jadejitsu.apps.Calendar` name. The blocking API drives its
//!   own message pump (a static tokio runtime inside zbus), so no executor
//!   plumbing is needed.
//! - [`activation_stream`] drains activations onto the iced update loop.
//! - [`activate_existing`] is called by `main` *before* launching: if another
//!   instance owns the name, we forward the activation to it and exit.
//!
//! The interface served must match what the COSMIC dock and libcosmic's
//! second-instance proxy expect: interface `org.freedesktop.DbusActivation`,
//! method `activate(a{sv})` (plus `open`/`activate_action` for file/URL
//! launches), path = `/` + app-id with dots replaced by slashes.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};

use futures_util::Stream;
use zbus::zvariant::Value;

use crate::message::Message;

/// The D-Bus interface served by the first instance.
const DBUS_ACTIVATION_INTERFACE: &str = "org.freedesktop.DbusActivation";

/// Events the first instance can deliver to the iced update loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivationEvent {
    /// Plain activation (dock-icon click, `gtk-launch`).
    Activate,
    /// Activation carrying URIs to open (second launch with file/URL args).
    Open { urls: Vec<String> },
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static TX: OnceLock<mpsc::Sender<ActivationEvent>> = OnceLock::new();
// `Mutex` makes the (Send-but-not-Sync) receiver storable in a static.
static RX: Mutex<Option<mpsc::Receiver<ActivationEvent>>> = Mutex::new(None);

/// The object path for an app-id: `/` + app-id with `.` → `/`.
/// `dev.jadejitsu.apps.Calendar` → `/dev/jadejitsu/apps/Calendar`.
pub fn object_path_for(app_id: &str) -> String {
    format!("/{}", app_id.replace('.', "/"))
}

/// Map an activation event to the `Message` the update loop should handle.
/// Plain activation reuses the tray "Show/Restore" path (focus the window if
/// open, reopen it if closed to the tray); `Open` processes the first target
/// — a scheme URL via the startup URL handler, a plain path via the import
/// handler (only one dialog can be open at a time, so the first wins).
pub fn activation_message(ev: &ActivationEvent) -> Message {
    match ev {
        ActivationEvent::Activate => Message::TrayShowOrRestore,
        ActivationEvent::Open { urls } => match urls.first() {
            Some(target) if target.contains("://") => Message::ProcessUrl(target.clone()),
            Some(target) => Message::ImportFile(std::path::PathBuf::from(target)),
            // No usable target: fall back to a plain restore.
            None => Message::TrayShowOrRestore,
        },
    }
}

/// Connect to the session bus and try to activate a running instance.
///
/// Returns `true` if another instance owns the app-id and accepted the
/// activation (the caller should exit), `false` if no instance is running (or
/// the bus is unavailable) and this process should become the first instance.
///
/// `urls` are forwarded via `open` when present, otherwise `activate` is
/// called — mirroring libcosmic's `run_single_instance` behavior.
pub fn activate_existing(app_id: &str, urls: &[String]) -> bool {
    let conn = match zbus::blocking::Connection::session() {
        Ok(conn) => conn,
        Err(e) => {
            log::warn!("single-instance: no session bus ({e}); continuing as first instance");
            return false;
        }
    };
    // Gate on *ownership*, not reachability. The app-id is registered as an
    // activatable service (res/*.service), so a plain `activate`/`open` call to
    // the unowned name would make the bus autostart a new instance of *this*
    // app — which would then probe first too, and both would block in
    // `call_method` forever. `NameHasOwner` is a read-only query that never
    // triggers activation, so it's safe to run before we know who owns the name.
    let owned = conn
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "NameHasOwner",
            &(app_id,),
        )
        .and_then(|reply| reply.body().deserialize::<bool>());
    match owned {
        Ok(true) => {}
        Ok(false) => {
            log::info!("single-instance: name {app_id} not owned; becoming first instance");
            return false;
        }
        Err(e) => {
            log::warn!("single-instance: NameHasOwner failed ({e}); becoming first instance");
            return false;
        }
    }

    let path_str = object_path_for(app_id);
    let path = match zbus::zvariant::ObjectPath::try_from(path_str.as_str()) {
        Ok(path) => path,
        Err(e) => {
            log::error!("single-instance: bad object path: {e}");
            return false;
        }
    };
    let mut platform_data: HashMap<&str, Value<'_>> = HashMap::new();
    if let Ok(token) = std::env::var("XDG_ACTIVATION_TOKEN") {
        platform_data.insert("activation-token", token.into());
    }
    if let Ok(startup_id) = std::env::var("DESKTOP_STARTUP_ID") {
        platform_data.insert("desktop-startup-id", startup_id.into());
    }

    // Note: the D-Bus member names are PascalCase (`Activate`/`Open`), matching
    // what the `#[zbus::interface]` macro exposes and what libcosmic's own
    // `DbusActivationInterfaceProxyBlocking` calls (its `assume_defaults` proxy
    // PascalCases `activate` → `Activate`). The COSMIC dock's launch goes through
    // the same proxy, so this is the wire name the dock uses.
    let result = if urls.is_empty() {
        conn.call_method(
            Some(app_id),
            &path,
            Some(DBUS_ACTIVATION_INTERFACE),
            "Activate",
            &(&platform_data,),
        )
    } else {
        conn.call_method(
            Some(app_id),
            &path,
            Some(DBUS_ACTIVATION_INTERFACE),
            "Open",
            &(urls, &platform_data),
        )
    };

    match result {
        Ok(_) => {
            log::info!("single-instance: activated existing instance, exiting");
            true
        }
        Err(e) => {
            log::info!("single-instance: no running instance ({e}); becoming first instance");
            false
        }
    }
}

/// Serve the D-Bus activation interface and own the app-id name.
///
/// Idempotent; safe to call from `CosmicCalendar::init`. Runs on a daemon
/// thread so a missing session bus can never block app startup.
pub fn init(app_id: &str) {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }

    let (tx, rx) = mpsc::channel::<ActivationEvent>();
    let _ = TX.set(tx);
    *RX.lock().unwrap() = Some(rx);

    let app_id = app_id.to_owned();
    std::thread::Builder::new()
        .name("single-instance".into())
        .spawn(move || serve(&app_id))
        .expect("single-instance: failed to spawn D-Bus thread");
}

/// The daemon-thread body: connect, serve, own the name.
///
/// Runs on a local tokio runtime with the *async* zbus API. The blocking API
/// can't be used here: `blocking::Connection::object_server()` is a sync call
/// that spawns the object-server task directly on the executor, so it panics
/// with "no reactor running" outside a runtime context (its `block_on` wrapper
/// only covers the async methods). A local multi-threaded runtime drives the
/// connection's reader + dispatch tasks for the process lifetime; holding `rt`
/// keeps them alive.
fn serve(app_id: &str) {
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_io()
        .enable_time()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            log::warn!(
                "single-instance: failed to build runtime ({e}); dock re-activation disabled"
            );
            return;
        }
    };

    rt.block_on(async move {
        let conn = match zbus::Connection::session().await {
            Ok(conn) => conn,
            Err(e) => {
                log::warn!("single-instance: no session bus ({e}); dock re-activation disabled");
                return;
            }
        };

        let iface = ActivationIface::new();
        let path_str = object_path_for(app_id);
        let path = match zbus::zvariant::ObjectPath::try_from(path_str.as_str()) {
            Ok(path) => path,
            Err(e) => {
                log::error!("single-instance: bad object path: {e}");
                return;
            }
        };
        if conn.object_server().at(&path, iface).await != Ok(true) {
            log::error!("single-instance: failed to serve {DBUS_ACTIVATION_INTERFACE} at {path}");
            return;
        }
        if let Err(e) = conn.request_name(app_id).await {
            log::error!("single-instance: failed to own name {app_id}: {e}");
            return;
        }
        log::info!("single-instance: owning {app_id} at {path}");

        // Park forever: the runtime's reader task keeps the connection (and
        // our interface) alive and dispatching until the process exits.
        std::future::pending::<()>().await;
    });
}

/// The `org.freedesktop.DbusActivation` implementation.
#[derive(Debug, Default)]
struct ActivationIface {
    tx: Option<mpsc::Sender<ActivationEvent>>,
}

impl ActivationIface {
    fn new() -> Self {
        Self {
            tx: TX.get().cloned(),
        }
    }
}

#[zbus::interface(name = "org.freedesktop.DbusActivation")]
impl ActivationIface {
    fn activate(&self, _platform_data: HashMap<&str, Value<'_>>) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(ActivationEvent::Activate);
        }
    }

    fn open(&self, uris: Vec<&str>, _platform_data: HashMap<&str, Value<'_>>) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(ActivationEvent::Open {
                urls: uris.into_iter().map(String::from).collect(),
            });
        }
    }

    fn activate_action(
        &self,
        _action_name: &str,
        _parameter: Vec<&str>,
        _platform_data: HashMap<&str, Value<'_>>,
    ) {
        // We have no GAction-style actions; treat as a plain activation.
        if let Some(tx) = &self.tx {
            let _ = tx.send(ActivationEvent::Activate);
        }
    }
}

/// iced subscription stream that drains the activation channel.
///
/// Same 250 ms poll shape as [`crate::services::tray::tray_event_stream`] —
/// imperceptible for a dock click, and avoids adding stream dependencies.
pub fn activation_stream() -> impl Stream<Item = Message> {
    futures_util::stream::unfold((), move |_| async move {
        loop {
            let ev = {
                let guard = RX.lock().unwrap();
                guard.as_ref().and_then(|rx| rx.try_recv().ok())
            };
            match ev {
                Some(ev) => return Some((activation_message(&ev), ())),
                None => {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_path_replaces_dots_with_slashes() {
        assert_eq!(
            object_path_for("dev.jadejitsu.apps.Calendar"),
            "/dev/jadejitsu/apps/Calendar"
        );
    }

    #[test]
    fn object_path_handles_single_segment() {
        assert_eq!(object_path_for("org.example"), "/org/example");
    }

    #[test]
    fn activate_maps_to_tray_show_or_restore() {
        let msg = activation_message(&ActivationEvent::Activate);
        assert!(matches!(msg, Message::TrayShowOrRestore));
    }

    #[test]
    fn open_maps_first_url_to_process_url() {
        let msg = activation_message(&ActivationEvent::Open {
            urls: vec!["webcal://example.com/cal.ics".into(), "ics://other".into()],
        });
        assert!(matches!(
            msg,
            Message::ProcessUrl(u) if u == "webcal://example.com/cal.ics"
        ));
    }

    #[test]
    fn open_maps_plain_path_to_import_file() {
        let msg = activation_message(&ActivationEvent::Open {
            urls: vec!["/home/user/events.ics".into()],
        });
        assert!(matches!(
            msg,
            Message::ImportFile(p) if p == std::path::Path::new("/home/user/events.ics")
        ));
    }

    #[test]
    fn open_with_no_urls_falls_back_to_restore() {
        let msg = activation_message(&ActivationEvent::Open { urls: vec![] });
        assert!(matches!(msg, Message::TrayShowOrRestore));
    }
}
