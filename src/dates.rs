//! Conversion helpers between the app's internal `chrono` dates and the
//! `jiff` dates the libcosmic `calendar()` widget uses (it migrated from
//! chrono to jiff). The app keeps its state in chrono and only converts at
//! the widget boundary.

/// Convert a `chrono::NaiveDate` to a `jiff::civil::Date`.
///
/// The widget boundary requires jiff; a valid `NaiveDate` is always a valid
/// jiff date, so the fallible constructor cannot actually fail here.
pub fn to_jiff(d: chrono::NaiveDate) -> jiff::civil::Date {
    use chrono::Datelike;
    jiff::civil::Date::new(d.year() as i16, d.month() as i8, d.day() as i8)
        .expect("valid NaiveDate is a valid jiff date")
}

/// Convert a `jiff::civil::Date` back to a `chrono::NaiveDate`.
pub fn from_jiff(d: jiff::civil::Date) -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(d.year() as i32, d.month() as u32, d.day() as u32)
        .expect("valid jiff date is a valid NaiveDate")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_preserves_date() {
        let original = chrono::NaiveDate::from_ymd_opt(2026, 9, 9).unwrap();
        let j = to_jiff(original);
        assert_eq!(from_jiff(j), original);
    }

    #[test]
    fn jiff_display_is_iso() {
        let j = to_jiff(chrono::NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
        assert_eq!(j.to_string(), "2026-01-05");
    }
}
