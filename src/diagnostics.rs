//! Debug-only flicker tracing.
//!
//! When `XCAL_FLICKER_TRACE` is set (path, or any non-empty value for the
//! default `/tmp/xcal-flicker-trace.log`), every `update()` dispatch and
//! every `view()` rebuild appends one line. This distinguishes
//! "input dispatches app messages" (many `[update]` lines per input burst)
//! from "iced redraws internally without app messages" (only `[view]`
//! lines, or neither — the latter meaning the redraw happens below the
//! MVU layer).
//!
//! Gated behind `debug_assertions` at the call sites; no cost in release.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

const DEFAULT_PATH: &str = "/tmp/xcal-flicker-trace.log";

fn trace_path() -> PathBuf {
    match std::env::var("XCAL_FLICKER_TRACE") {
        Ok(p) if !p.is_empty() => PathBuf::from(p),
        _ => PathBuf::from(DEFAULT_PATH),
    }
}

fn append_line(path: &std::path::Path, line: &str) {
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

pub fn trace_message(message: &crate::message::Message) {
    append_line(&trace_path(), &format!("[update] {message:?}"));
}

pub fn trace_view_rebuild() {
    append_line(&trace_path(), "[view]");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Message;

    #[test]
    fn appends_update_line_with_message_debug() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("xcal-trace-test-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);

        append_line(&path, "[update] ToggleContextDrawer");

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "[update] ToggleContextDrawer\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn appends_are_cumulative() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("xcal-trace-test2-{}.log", std::process::id()));
        let _ = std::fs::remove_file(&path);

        append_line(&path, "[view]");
        append_line(&path, "[update] ToggleContextDrawer");

        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "[view]\n[update] ToggleContextDrawer\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn trace_message_writes_debug_repr() {
        let path = trace_path();
        let _ = std::fs::remove_file(&path);
        trace_message(&Message::ToggleContextDrawer);
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("ToggleContextDrawer"));
        let _ = std::fs::remove_file(&path);
    }
}
