//! Cached view-state models (month/week/day/year) computed once per
//! displayed period to avoid recalculating on every render.

mod calendar_state;
mod day_state;
mod week_state;
mod year_state;

pub use calendar_state::{CalendarDay, CalendarState};
pub use day_state::DayState;
pub use week_state::WeekState;
pub use year_state::YearState;
