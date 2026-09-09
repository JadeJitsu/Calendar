//! Notification scheduler — decides which event alerts are due "now" and
//! when the next one will be due.
//!
//! Pure, time-injected logic:
//! - `due_notifications(now, events)` returns the alerts whose due instant
//!   (occurrence start − alert offset) falls inside the trailing due-window.
//! - `next_due_time(now, events)` returns the earliest *future* due instant,
//!   so the app can arm a precise one-shot timer to it (tight timing) rather
//!   than waiting for the next 30-second tick.
//!
//! The `fired` set deduplicates so each (occurrence, alert) fires once.

use std::collections::HashSet;

use chrono::{DateTime, Duration, NaiveDate, Utc};

use crate::caldav::{alert_minutes, CalendarEvent, RepeatFrequency};
use crate::calendars::CalendarManager;

/// How far behind "now" an alert's due instant can be and still be delivered.
/// Comfortably exceeds the tick interval (30 s) so a tick never skips a due
/// alert, while staying small enough that a stale alert isn't fired minutes
/// late.
pub const DUE_WINDOW: Duration = Duration::seconds(60);

/// How far ahead `next_due_time` looks for the next upcoming alert. Generous:
/// covers the largest fixed offset (OneWeek) plus `Custom`, and recurring
/// events whose next occurrence may be days away.
pub const SCHEDULE_HORIZON: Duration = Duration::days(366);

/// An alert that is due right now.
#[derive(Debug, Clone, PartialEq)]
pub struct DueNotification {
    /// The (occurrence-adjusted) event to notify about.
    pub event: CalendarEvent,
    /// The start instant of the specific occurrence this alert belongs to.
    pub occurrence_start: DateTime<Utc>,
    /// Which alert fired: 0 = primary, 1 = secondary.
    pub alert_index: usize,
}

/// Tracks which alerts have already fired so each fires exactly once.
#[derive(Debug, Default)]
pub struct NotificationScheduler {
    /// `(uid, occurrence_start, alert_index)` keys that have been delivered.
    fired: HashSet<String>,
}

impl NotificationScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    /// Key for a specific (occurrence, alert) pair.
    fn key(event: &CalendarEvent, occurrence_start: DateTime<Utc>, alert_index: usize) -> String {
        format!(
            "{}|{}|{}",
            event.uid,
            occurrence_start.format("%Y%m%dT%H%M%SZ"),
            alert_index
        )
    }

    /// All alerts due within `[now - DUE_WINDOW, now]` that have not fired
    /// yet. Non-recurring events are checked directly; recurring events are
    /// expanded over the window and each occurrence is checked on its own
    /// start time.
    pub fn due_notifications(
        &mut self,
        now: DateTime<Utc>,
        events: &[CalendarEvent],
    ) -> Vec<DueNotification> {
        let mut due = Vec::new();
        for (_, occurrence_start, occurrence, index) in
            self.alert_due_instants(events, now - DUE_WINDOW, now)
        {
            let key = Self::key(&occurrence, occurrence_start, index);
            if self.fired.insert(key) {
                due.push(DueNotification {
                    event: occurrence,
                    occurrence_start,
                    alert_index: index,
                });
            }
        }
        due
    }

    /// The earliest due instant strictly after `now`, or `None` if no alert
    /// is scheduled within `SCHEDULE_HORIZON`. Used to arm a precise one-shot
    /// timer so an alert fires at its exact due time instead of on the next
    /// tick.
    pub fn next_due_time(
        &self,
        now: DateTime<Utc>,
        events: &[CalendarEvent],
    ) -> Option<DateTime<Utc>> {
        let horizon = now + SCHEDULE_HORIZON;
        self.alert_due_instants(events, now, horizon)
            .into_iter()
            .map(|(due_at, _, _, _)| due_at)
            .min()
    }

    /// Every alert due-instant for `events` whose due time falls in
    /// `[range_start, range_end]`, as `(due_at, occurrence_start, occurrence,
    /// alert_index)`. Shared by `due_notifications` (a trailing window) and
    /// `next_due_time` (a leading horizon).
    fn alert_due_instants(
        &self,
        events: &[CalendarEvent],
        range_start: DateTime<Utc>,
        range_end: DateTime<Utc>,
    ) -> Vec<(DateTime<Utc>, DateTime<Utc>, CalendarEvent, usize)> {
        let mut out = Vec::new();
        for event in events {
            let mut alerts: Vec<(usize, Option<i64>)> =
                vec![(0usize, alert_minutes(&event.alert))];
            if let Some(second) = &event.alert_second {
                alerts.push((1usize, alert_minutes(second)));
            }
            for (occurrence_start, occurrence) in
                self.occurrences_in_range(event, range_start, range_end)
            {
                for (index, offset) in &alerts {
                    let Some(mins) = offset else {
                        continue;
                    };
                    let due_at = occurrence_start - Duration::minutes(*mins);
                    if range_start <= due_at && due_at <= range_end {
                        out.push((due_at, occurrence_start, occurrence.clone(), *index));
                    }
                }
            }
        }
        out
    }

    /// Occurrences of `event` whose alert could fall inside
    /// `[range_start, range_end]`: `(occurrence_start, event_with_adjusted_dates)`.
    fn occurrences_in_range(
        &self,
        event: &CalendarEvent,
        range_start: DateTime<Utc>,
        range_end: DateTime<Utc>,
    ) -> Vec<(DateTime<Utc>, CalendarEvent)> {
        // An alert for an occurrence starting at S is due in the range when
        // S - offset ∈ [range_start, range_end], i.e. S ∈ [range_start,
        // range_end + max_offset]. Max offset is OneWeek (the largest fixed
        // variant); Custom can be larger, so widen the range generously.
        let max_offset = Duration::days(30);
        let date_start = range_start.date_naive();
        let date_end = (range_end + max_offset).date_naive();

        if matches!(event.repeat, RepeatFrequency::Never) {
            let start_date = event.start.date_naive();
            if (date_start..=date_end).contains(&start_date) {
                return vec![(event.start, event.clone())];
            }
            return vec![];
        }

        // expand_recurring_event already returns each occurrence with
        // start/end adjusted to that date and a per-date uid.
        CalendarManager::expand_recurring_event(event, date_start, date_end)
            .into_iter()
            .map(|(_, occ)| (occ.start, occ))
            .collect()
    }

    /// Drop fired keys for occurrences that can no longer be due (keeps the
    /// set bounded as the app runs for days).
    pub fn prune(&mut self, now: DateTime<Utc>) {
        let cutoff = format!("{}", (now - DUE_WINDOW).format("%Y%m%dT%H%M%SZ"));
        self.fired.retain(|k| {
            k.rsplit('|')
                .nth(1)
                .map(|ts| ts >= cutoff.as_str())
                .unwrap_or(false)
        });
    }
}

/// Desktop notification payloads for a set of due alerts: `(title, body)`.
///
/// Kept separate from the actual `notify-rust` send so the message content
/// is unit-testable without a notification daemon.
pub fn notification_payloads(due: &[DueNotification]) -> Vec<(String, String)> {
    due.iter()
        .map(|n| {
            let mut body = format!("Starts at {}", n.occurrence_start.format("%H:%M"));
            if let Some(loc) = &n.event.location {
                body.push_str(&format!("\n{}", loc));
            }
            (n.event.summary.clone(), body)
        })
        .collect()
}

/// Fire desktop notifications for due alerts. Returns the payloads that
/// were sent (empty if there was nothing due).
pub fn fire_notifications(due: &[DueNotification]) -> Vec<(String, String)> {
    let payloads = notification_payloads(due);
    for (title, body) in &payloads {
        notify_rust::Notification::new()
            .summary(title)
            .body(body)
            .appname("Calendar")
            .show()
            .ok(); // No daemon / headless session: drop silently.
    }
    payloads
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use crate::caldav::AlertTime;

    fn event_at(start: DateTime<Utc>, alert: AlertTime) -> CalendarEvent {
        CalendarEvent {
            uid: "evt-1".to_string(),
            summary: "Test".to_string(),
            location: None,
            all_day: false,
            start,
            end: start + Duration::hours(1),
            travel_time: Default::default(),
            repeat: RepeatFrequency::Never,
            repeat_until: None,
            exception_dates: vec![],
            invitees: vec![],
            alert,
            alert_second: None,
            attachments: vec![],
            url: None,
            notes: None,
        }
    }

    fn at(h: u32, m: u32, s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 9, h, m, s).unwrap()
    }

    #[test]
    fn fires_when_due_instant_is_inside_window() {
        let mut sched = NotificationScheduler::new();
        let event = event_at(at(12, 0, 0), AlertTime::FifteenMinutes);
        // Due at 11:45:00; now = 11:45:30 → inside the 60 s window.
        let due = sched.due_notifications(at(11, 45, 30), &[event.clone()]);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].event.uid, "evt-1");
        assert_eq!(due[0].occurrence_start, at(12, 0, 0));
        assert_eq!(due[0].alert_index, 0);
    }

    #[test]
    fn does_not_fire_before_due_instant() {
        let mut sched = NotificationScheduler::new();
        let event = event_at(at(12, 0, 0), AlertTime::FifteenMinutes);
        // Due at 11:45:00; now = 11:44:00 → not due yet.
        assert!(sched.due_notifications(at(11, 44, 0), &[event]).is_empty());
    }

    #[test]
    fn does_not_fire_after_window() {
        let mut sched = NotificationScheduler::new();
        let event = event_at(at(12, 0, 0), AlertTime::FifteenMinutes);
        // Due at 11:45:00; now = 11:47:00 → 90 s late, outside window.
        assert!(sched.due_notifications(at(11, 47, 0), &[event]).is_empty());
    }

    #[test]
    fn none_alert_never_fires() {
        let mut sched = NotificationScheduler::new();
        let event = event_at(at(12, 0, 0), AlertTime::None);
        assert!(sched.due_notifications(at(11, 45, 30), &[event]).is_empty());
    }

    #[test]
    fn dedupes_within_same_scheduler() {
        let mut sched = NotificationScheduler::new();
        let event = event_at(at(12, 0, 0), AlertTime::FifteenMinutes);
        assert_eq!(sched.due_notifications(at(11, 45, 30), &[event.clone()]).len(), 1);
        // Second tick in the same window must not re-fire.
        assert!(sched.due_notifications(at(11, 45, 50), &[event]).is_empty());
    }

    #[test]
    fn next_due_time_returns_earliest_future_due() {
        let sched = NotificationScheduler::new();
        let event = event_at(at(12, 0, 0), AlertTime::FifteenMinutes);
        // Due at 11:45:00; now = 11:00 → next due is 11:45.
        assert_eq!(
            sched.next_due_time(at(11, 0, 0), &[event]),
            Some(at(11, 45, 0))
        );
    }

    #[test]
    fn next_due_time_none_when_already_past() {
        let sched = NotificationScheduler::new();
        let event = event_at(at(12, 0, 0), AlertTime::FifteenMinutes);
        // Due at 11:45:00; now = 12:00 → already past, nothing future.
        assert_eq!(sched.next_due_time(at(12, 0, 0), &[event]), None);
    }

    #[test]
    fn next_due_time_picks_earliest_of_two_alerts() {
        let sched = NotificationScheduler::new();
        let mut event = event_at(at(12, 0, 0), AlertTime::OneHour);
        event.alert_second = Some(AlertTime::FifteenMinutes);
        // 1h due 11:00, 15m due 11:45; now = 10:00 → earliest is 11:00.
        assert_eq!(
            sched.next_due_time(at(10, 0, 0), &[event]),
            Some(at(11, 0, 0))
        );
    }

    #[test]
    fn next_due_time_none_when_no_alerts() {
        let sched = NotificationScheduler::new();
        let event = event_at(at(12, 0, 0), AlertTime::None);
        assert_eq!(sched.next_due_time(at(11, 0, 0), &[event]), None);
    }

    #[test]
    fn next_due_time_recurring_next_occurrence() {
        let sched = NotificationScheduler::new();
        let mut event = event_at(at(9, 0, 0), AlertTime::FifteenMinutes);
        event.repeat = RepeatFrequency::Daily;
        // now = 2026-09-09 12:00 → the 9th's 08:45 is past; next is the
        // 10th's 08:45.
        let now = Utc.with_ymd_and_hms(2026, 9, 9, 12, 0, 0).unwrap();
        assert_eq!(
            sched.next_due_time(now, &[event]),
            Some(Utc.with_ymd_and_hms(2026, 9, 10, 8, 45, 0).unwrap())
        );
    }

    #[test]
    fn secondary_alert_fires_independently() {
        let mut sched = NotificationScheduler::new();
        let mut event = event_at(at(12, 0, 0), AlertTime::OneHour);
        event.alert_second = Some(AlertTime::FifteenMinutes);
        // At 11:00:30 the 1h alert is due (11:00:00); the 15m one isn't yet.
        let due = sched.due_notifications(at(11, 0, 30), &[event.clone()]);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].alert_index, 0);
        // At 11:45:30 the 15m alert is due; the 1h one already fired.
        let due = sched.due_notifications(at(11, 45, 30), &[event]);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].alert_index, 1);
    }

    #[test]
    fn recurring_event_fires_per_occurrence() {
        let mut sched = NotificationScheduler::new();
        let mut event = event_at(at(9, 0, 0), AlertTime::FifteenMinutes);
        event.repeat = RepeatFrequency::Daily;
        // Day 1: due 08:45:00, now 08:45:30 → fires for the 9th.
        let d1 = Utc.with_ymd_and_hms(2026, 9, 9, 8, 45, 30).unwrap();
        let due = sched.due_notifications(d1, &[event.clone()]);
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].occurrence_start, at(9, 0, 0));
        // Same tick must not double-fire the same occurrence.
        assert!(sched.due_notifications(d1, &[event.clone()]).is_empty());
        // Day 2: a different occurrence fires independently.
        let d2 = Utc.with_ymd_and_hms(2026, 9, 10, 8, 45, 30).unwrap();
        let due = sched.due_notifications(d2, &[event]);
        assert_eq!(due.len(), 1);
        assert_eq!(
            due[0].occurrence_start,
            Utc.with_ymd_and_hms(2026, 9, 10, 9, 0, 0).unwrap()
        );
    }

    #[test]
    fn recurring_event_respects_exception_dates() {
        let mut sched = NotificationScheduler::new();
        let mut event = event_at(at(9, 0, 0), AlertTime::FifteenMinutes);
        event.repeat = RepeatFrequency::Daily;
        event.exception_dates = vec![NaiveDate::from_ymd_opt(2026, 9, 9).unwrap()];
        // The 9th is excepted; nothing due.
        let d1 = Utc.with_ymd_and_hms(2026, 9, 9, 8, 45, 30).unwrap();
        assert!(sched.due_notifications(d1, &[event.clone()]).is_empty());
        // The 10th still fires.
        let d2 = Utc.with_ymd_and_hms(2026, 9, 10, 8, 45, 30).unwrap();
        assert_eq!(sched.due_notifications(d2, &[event]).len(), 1);
    }

    #[test]
    fn fire_notifications_builds_desktop_payloads() {
        let mut sched = NotificationScheduler::new();
        let event = event_at(at(12, 0, 0), AlertTime::FifteenMinutes);
        let due = sched.due_notifications(at(11, 45, 30), &[event]);
        let fired = fire_notifications(&due);
        assert_eq!(fired.len(), 1);
        // Title is the event summary; body carries the start time.
        assert_eq!(fired[0].0, "Test");
        assert!(fired[0].1.contains("12:00"));
    }

    #[test]
    fn fire_notifications_empty_for_no_due() {
        assert!(fire_notifications(&[]).is_empty());
    }

    #[test]
    fn recurring_event_respects_repeat_until() {
        let mut sched = NotificationScheduler::new();
        let mut event = event_at(at(9, 0, 0), AlertTime::FifteenMinutes);
        event.repeat = RepeatFrequency::Daily;
        event.repeat_until = Some(NaiveDate::from_ymd_opt(2026, 9, 9).unwrap());
        // Recurrence ended on the 9th → the 10th must not fire.
        let d2 = Utc.with_ymd_and_hms(2026, 9, 10, 8, 45, 30).unwrap();
        assert!(sched.due_notifications(d2, &[event]).is_empty());
    }
}
