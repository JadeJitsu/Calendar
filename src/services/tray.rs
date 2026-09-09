//! System tray icon (StatusNotifierItem on Wayland/COSMIC).
//!
//! The `tray-icon` crate can't touch iced state directly, so menu clicks are
//! pushed onto an `mpsc` channel that a iced `Subscription`
//! (see [`tray_event_stream`]) drains into `Message` values on the UI thread.
//!
//! **Linux requires a GTK main loop.** The Linux backend is libappindicator
//! (GTK3), and the StatusNotifierItem only registers once a GTK loop is
//! running on the thread that created the icon. iced/winit never pumps GTK, so
//! [`init`] spawns a dedicated thread that calls `gtk::init()`, builds the
//! icon, and runs `gtk::main()` for the process lifetime (mirroring
//! `tray-icon`'s own `winit.rs` example). Without that loop the icon silently
//! never appears in the panel — no error, just an empty tray.
//!
//! The `TrayIcon` must live for the process lifetime (dropping it removes the
//! icon). It is `!Send + !Sync` (`Rc<RefCell<..>>` internally), so it can't be
//! held in a `static` — we leak it with `mem::forget`, which is exactly the
//! "live forever" semantics we want.

use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock};

use futures_util::Stream;
use rust_embed::RustEmbed;
use tray_icon::menu::{Menu, MenuItem, MenuEvent};
use tray_icon::{Icon, TrayIconBuilder};

use crate::fl;
use crate::message::Message;

/// Events the tray can deliver to the iced update loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    /// "Show/Restore" menu item clicked.
    ShowOrRestore,
    /// "Quit" menu item clicked.
    Quit,
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static TX: OnceLock<mpsc::Sender<TrayEvent>> = OnceLock::new();
// `Mutex` makes the (Send-but-not-Sync) receiver storable in a static.
static RX: Mutex<Option<mpsc::Receiver<TrayEvent>>> = Mutex::new(None);

/// The bundled tray icon (64×64 PNG, decoded to RGBA at runtime).
#[derive(RustEmbed)]
#[folder = "res/icons/hicolor/64x64/apps/"]
struct TrayIconAssets;

/// Build the tray icon and its menu once (idempotent). Call from
/// `CosmicCalendar::init`. Safe to call more than once — subsequent calls
/// are no-ops.
pub fn init() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }

    let (tx, rx) = mpsc::channel::<TrayEvent>();
    let _ = TX.set(tx);
    *RX.lock().unwrap() = Some(rx);

    // On Linux the tray backend is libappindicator (GTK3): the StatusNotifierItem
    // only registers once a GTK main loop is running on the thread that created
    // it. iced/winit doesn't pump GTK, so we spawn a dedicated thread that inits
    // GTK, builds the icon, and runs `gtk::main()` (see tray-icon's winit.rs
    // example). On other platforms the icon is built on the calling thread.
    #[cfg(target_os = "linux")]
    {
        std::thread::Builder::new()
            .name("tray-gtk".into())
            .spawn(build_tray_and_run_gtk)
            .expect("tray: failed to spawn GTK thread");
    }
    #[cfg(not(target_os = "linux"))]
    {
        build_tray();
    }
}

/// Build the tray icon and its menu, then install the global menu-event handler.
/// On Linux this runs on the GTK thread (after `gtk::init()`).
fn build_tray() {
    let show = MenuItem::new(fl!("tray-menu-show"), true, None);
    let quit = MenuItem::new(fl!("tray-menu-quit"), true, None);
    let menu = Menu::new();
    if let Err(e) = menu.append_items(&[&show, &quit]) {
        log::error!("tray: failed to build menu: {}", e);
        return;
    }
    // Capture the (cheap, Send+Sync) ids rather than the MenuItems, which are
    // !Send and can't be moved into the event handler closure.
    let show_id = show.id().clone();
    let quit_id = quit.id().clone();

    // On Linux the icon will not appear unless a menu is attached — we always
    // attach one.
    let icon = match load_icon() {
        Some(icon) => icon,
        None => return,
    };
    let tray = match TrayIconBuilder::new()
        .with_icon(icon)
        .with_menu(Box::new(menu))
        .build()
    {
        Ok(tray) => tray,
        Err(e) => {
            log::error!("tray: failed to build tray icon: {}", e);
            return;
        }
    };

    // Leak the TrayIcon: it must persist for the process lifetime and can't be
    // stored in a static (it is !Send + !Sync).
    std::mem::forget(tray);

    // Route menu clicks (delivered on the tray thread) onto the channel.
    // `MenuEvent` is passed by value.
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let ev = if e.id() == &show_id {
            TrayEvent::ShowOrRestore
        } else if e.id() == &quit_id {
            TrayEvent::Quit
        } else {
            return;
        };
        if let Some(t) = TX.get() {
            let _ = t.send(ev);
        }
    }));
}

/// Linux-only: init GTK on this thread, build the tray, then pump the GTK loop
/// for the process lifetime.
#[cfg(target_os = "linux")]
fn build_tray_and_run_gtk() {
    if let Err(e) = gtk::init() {
        log::error!("tray: failed to init GTK: {}", e);
        return;
    }
    build_tray();
    gtk::main();
}

/// iced subscription stream that drains the tray channel.
///
/// Polls the receiver on a 250 ms interval — imperceptible for a menu click,
/// and avoids adding `async-stream`/`futures-channel` dependencies. The mutex
/// guard is dropped before any `await`, so it is never held across a yield.
pub fn tray_event_stream() -> impl Stream<Item = Message> {
    futures_util::stream::unfold((), move |_| async move {
        loop {
            let ev = {
                let guard = RX.lock().unwrap();
                guard.as_ref().and_then(|rx| rx.try_recv().ok())
            };
            match ev {
                Some(TrayEvent::ShowOrRestore) => {
                    return Some((Message::TrayShowOrRestore, ()));
                }
                Some(TrayEvent::Quit) => return Some((Message::TrayQuit, ())),
                None => {
                    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
                }
            }
        }
    })
}

/// Decode the bundled PNG into an `Icon` (RGBA). Returns `None` (and logs) on
/// any failure so a bad/missing asset can't crash the app at startup.
fn load_icon() -> Option<Icon> {
    let file = TrayIconAssets::get("dev.xarbit.apps.Calendar.png")?;
    // `EmbeddedFile.data` is a `Cow<'static, [u8]>`.
    let bytes: &[u8] = &file.data;

    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    // Normalize the output to 8-bit RGBA regardless of the source color type
    // (the bundled asset is RGBA, but this keeps it robust to re-encoding).
    decoder.set_transformations(png::Transformations::EXPAND
        | png::Transformations::ALPHA
        | png::Transformations::STRIP_16);
    let mut reader = match decoder.read_info() {
        Ok(reader) => reader,
        Err(e) => {
            log::error!("tray: failed to read PNG header: {}", e);
            return None;
        }
    };
    let (w, h) = reader.info().size();
    let mut rgba = Vec::with_capacity(reader.output_buffer_size());
    while let Some(row) = match reader.next_row() {
        Ok(row) => row,
        Err(e) => {
            log::error!("tray: failed to decode PNG: {}", e);
            return None;
        }
    } {
        rgba.extend_from_slice(row.data());
    }

    match Icon::from_rgba(rgba, w, h) {
        Ok(icon) => Some(icon),
        Err(e) => {
            log::error!("tray: invalid RGBA for icon: {}", e);
            None
        }
    }
}
