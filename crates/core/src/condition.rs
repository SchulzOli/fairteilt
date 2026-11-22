//! Condition of a task (KONZEPT §7.1): how much of the current round has passed.
//! 0 … < 0.75 fresh (green), 0.75 … < 1 due (honey), ≥ 1 overdue (red).
//! XP escalation starts with "overdue" (see `PointPolicy::current_points`, age = now − due).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const DUE_THRESHOLD: f64 = 0.75;

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionState {
    Fresh,
    Due,
    Overdue,
}

/// Share of the round between `start` and `due` that has passed at `now`.
/// Tasks without a due date stay fresh (0).
pub fn condition(start: DateTime<Utc>, due: Option<DateTime<Utc>>, now: DateTime<Utc>) -> f64 {
    let Some(due) = due else { return 0.0 };
    let total = (due - start).num_seconds();
    if total <= 0 {
        return if now >= due { 1.0 } else { 0.0 };
    }
    ((now - start).num_seconds() as f64 / total as f64).max(0.0)
}

pub fn state(value: f64) -> ConditionState {
    if value >= 1.0 {
        ConditionState::Overdue
    } else if value >= DUE_THRESHOLD {
        ConditionState::Due
    } else {
        ConditionState::Fresh
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn bathroom_every_seven_days() {
        let start = Utc::now() - Duration::days(30);
        let due = Some(start + Duration::days(7));
        let at = |d: i64| condition(start, due, start + Duration::days(d));
        assert_eq!(state(at(2)), ConditionState::Fresh);
        assert_eq!(state(at(6)), ConditionState::Due, "yellow after six days");
        assert_eq!(state(at(8)), ConditionState::Overdue, "red after eight days");
        assert!((at(14) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn no_due_date_stays_fresh() {
        let now = Utc::now();
        assert_eq!(condition(now - Duration::days(100), None, now), 0.0);
    }

    #[test]
    fn degenerate_round() {
        let t = Utc::now();
        assert_eq!(condition(t, Some(t), t), 1.0);
        assert_eq!(condition(t, Some(t), t - Duration::seconds(1)), 0.0);
    }
}
