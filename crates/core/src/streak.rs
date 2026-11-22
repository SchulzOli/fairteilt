//! Streak (KONZEPT §6): days in a row on which a member completed at least one task.
//!
//! A streak stays alive through today: if nothing is done yet today, yesterday's run still
//! counts (the day is not over). It breaks only when a whole day passes without a task.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Streak {
    /// Days in a row, ending today or yesterday.
    pub days: u32,
    /// Something was done today (the streak is safe for today).
    pub today: bool,
}

/// `days_done`: local dates with at least one completed task, in any order, duplicates allowed.
pub fn streak(days_done: &[NaiveDate], today: NaiveDate) -> Streak {
    let mut days: Vec<NaiveDate> = days_done.iter().copied().filter(|d| *d <= today).collect();
    days.sort_unstable();
    days.dedup();
    let done_today = days.last() == Some(&today);
    let mut expected = if done_today { today } else { today.pred_opt().unwrap_or(today) };
    let mut count = 0;
    for d in days.iter().rev() {
        if *d == expected {
            count += 1;
            expected = expected.pred_opt().unwrap_or(expected);
        } else if *d < expected {
            break;
        }
    }
    Streak { days: count, today: done_today }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, day).unwrap()
    }

    #[test]
    fn counts_days_in_a_row_until_today() {
        let s = streak(&[d(20), d(22), d(21), d(22)], d(22));
        assert_eq!(s, Streak { days: 3, today: true });
    }

    #[test]
    fn yesterday_keeps_the_streak_alive() {
        assert_eq!(streak(&[d(20), d(21)], d(22)), Streak { days: 2, today: false });
    }

    #[test]
    fn a_missed_day_breaks_it() {
        assert_eq!(streak(&[d(18), d(19), d(20)], d(22)), Streak { days: 0, today: false });
        assert_eq!(streak(&[d(18), d(20), d(21), d(22)], d(22)), Streak { days: 3, today: true });
    }

    #[test]
    fn empty_and_month_boundary() {
        assert_eq!(streak(&[], d(22)), Streak { days: 0, today: false });
        let oct31 = NaiveDate::from_ymd_opt(2026, 10, 31).unwrap();
        assert_eq!(streak(&[oct31, d(1)], d(1)).days, 2);
    }
}
