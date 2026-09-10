//! Event search: case-insensitive substring match across an event's
//! summary, location, notes, and invitee list.
//!
//! Pure and UI-agnostic so it can be unit-tested without a running app. The
//! caller supplies each event together with its owning calendar id and color
//! (search results are shown across all enabled calendars).

use crate::caldav::CalendarEvent;

/// Which field a result matched on (first match wins, in display priority
/// order: summary, location, notes, invitee).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchedField {
    Summary,
    Location,
    Notes,
    Invitee,
}

/// One search hit. Carries just enough to render a row and to open the event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    pub calendar_id: String,
    pub uid: String,
    pub summary: String,
    pub location: Option<String>,
    pub start: chrono::DateTime<chrono::Utc>,
    pub color: String,
    pub matched_field: MatchedField,
}

/// Search `events` for `query`.
///
/// `events` is an iterator of `(calendar_id, color, event)` tuples. Matching
/// is case-insensitive substring over summary, location, notes, and each
/// invitee's email. An empty/whitespace-only query returns no results.
/// Results preserve input order.
pub fn search_events<'a>(
    query: &str,
    events: impl IntoIterator<Item = (&'a str, &'a str, &'a CalendarEvent)>,
) -> Vec<SearchResult> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    events
        .into_iter()
        .filter_map(|(calendar_id, color, event)| {
            // First match wins, in display priority order.
            let matched_field = if event.summary.to_lowercase().contains(&needle) {
                Some(MatchedField::Summary)
            } else if event
                .location
                .as_deref()
                .map(|l| l.to_lowercase().contains(&needle))
                .unwrap_or(false)
            {
                Some(MatchedField::Location)
            } else if event
                .notes
                .as_deref()
                .map(|n| n.to_lowercase().contains(&needle))
                .unwrap_or(false)
            {
                Some(MatchedField::Notes)
            } else if event
                .invitees
                .iter()
                .any(|i| i.to_lowercase().contains(&needle))
            {
                Some(MatchedField::Invitee)
            } else {
                None
            }?;

            Some(SearchResult {
                calendar_id: calendar_id.to_string(),
                uid: event.uid.clone(),
                summary: event.summary.clone(),
                location: event.location.clone(),
                start: event.start,
                color: color.to_string(),
                matched_field,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caldav::{AlertTime, RepeatFrequency, TravelTime};
    use chrono::{TimeZone, Utc};

    fn event(uid: &str, summary: &str) -> CalendarEvent {
        CalendarEvent {
            uid: uid.to_string(),
            summary: summary.to_string(),
            location: None,
            all_day: false,
            start: Utc.with_ymd_and_hms(2026, 9, 10, 9, 0, 0).unwrap(),
            end: Utc.with_ymd_and_hms(2026, 9, 10, 10, 0, 0).unwrap(),
            travel_time: TravelTime::None,
            repeat: RepeatFrequency::Never,
            repeat_until: None,
            exception_dates: vec![],
            invitees: vec![],
            alert: AlertTime::None,
            alert_second: None,
            attachments: vec![],
            url: None,
            notes: None,
        }
    }

    #[test]
    fn empty_query_returns_nothing() {
        let e = event("e1", "Standup");
        assert!(search_events("", [("cal", "#fff", &e)]).is_empty());
        assert!(search_events("   ", [("cal", "#fff", &e)]).is_empty());
    }

    #[test]
    fn no_match_returns_nothing() {
        let e = event("e1", "Standup");
        assert!(search_events("lunch", [("cal", "#fff", &e)]).is_empty());
    }

    #[test]
    fn matches_summary_case_insensitively() {
        let e = event("e1", "Team Standup");
        let res = search_events("stand", [("cal", "#ff0000", &e)]);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].uid, "e1");
        assert_eq!(res[0].calendar_id, "cal");
        assert_eq!(res[0].color, "#ff0000");
        assert_eq!(res[0].matched_field, MatchedField::Summary);
    }

    #[test]
    fn matches_location() {
        let mut e = event("e1", "Dinner");
        e.location = Some("The Blue Note".to_string());
        let res = search_events("blue note", [("cal", "#00ff00", &e)]);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].matched_field, MatchedField::Location);
        assert_eq!(res[0].location.as_deref(), Some("The Blue Note"));
    }

    #[test]
    fn matches_notes() {
        let mut e = event("e1", "Call");
        e.notes = Some("re: the Q3 budget review".to_string());
        let res = search_events("budget", [("cal", "#0000ff", &e)]);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].matched_field, MatchedField::Notes);
    }

    #[test]
    fn matches_invitee_email() {
        let mut e = event("e1", "1:1");
        e.invitees = vec!["alice@example.com".to_string()];
        let res = search_events("alice", [("cal", "#000000", &e)]);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].matched_field, MatchedField::Invitee);
    }

    #[test]
    fn summary_takes_priority_over_other_fields() {
        let mut e = event("e1", "alpha meeting");
        e.notes = Some("alpha in the notes too".to_string());
        let res = search_events("alpha", [("cal", "#123456", &e)]);
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].matched_field, MatchedField::Summary);
    }

    #[test]
    fn multiple_events_preserve_input_order() {
        let a = event("a", "Zebra");
        let b = event("b", "apple pie");
        let c = event("c", "Apple tart");
        let res = search_events(
            "apple",
            [
                ("cal", "#fff", &a),
                ("cal", "#fff", &b),
                ("cal", "#fff", &c),
            ],
        );
        assert_eq!(
            res.iter().map(|r| r.uid.as_str()).collect::<Vec<_>>(),
            vec!["b", "c"]
        );
    }
}
