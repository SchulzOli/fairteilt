//! Periods and the family goal: a race against the mascot Krümel (KONZEPT §6).
//!
//! The family collects XP together. Krümel walks evenly from 0 to the goal and arrives exactly
//! when the period ends; every overdue task pushes him a bit further (a share of its XP per
//! overdue day). Whoever reaches the goal first decides the family reward.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};

use crate::Points;
use crate::settlement::PeriodLength;

/// Family goal settings, stored as JSON on the household.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalPolicy {
    /// Fixed goal per period; `None` = suggested from the last period.
    pub target: Option<Points>,
    /// What the family gets when it wins ("Eis essen", "Ausflug").
    pub reward: String,
    /// Share of an overdue task's XP that pushes Krümel per overdue day, in percent.
    pub boost_percent: u32,
}

impl Default for GoalPolicy {
    fn default() -> Self {
        Self { target: None, reward: "Familienausflug".into(), boost_percent: 25 }
    }
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RaceStatus {
    Running,
    /// The family reached the goal before Krümel: the reward is theirs.
    Won,
    /// Krümel got there first (at the latest when the period ended).
    Lost,
}

impl RaceStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RaceStatus::Running => "running",
            RaceStatus::Won => "won",
            RaceStatus::Lost => "lost",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "won" => RaceStatus::Won,
            "lost" => RaceStatus::Lost,
            _ => RaceStatus::Running,
        }
    }
}

/// Local 00:00 of a day as UTC (DST gaps: the earliest valid time).
pub fn local_midnight(date: NaiveDate, tz: Tz) -> DateTime<Utc> {
    let naive = date.and_hms_opt(0, 0, 0).expect("midnight exists");
    tz.from_local_datetime(&naive)
        .earliest()
        .unwrap_or_else(|| tz.from_utc_datetime(&naive))
        .with_timezone(&Utc)
}

fn add_months(date: NaiveDate, months: u32) -> NaiveDate {
    let total = date.year() * 12 + date.month0() as i32 + months as i32;
    let (y, m) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    let last = (28..=31).rev().find(|d| NaiveDate::from_ymd_opt(y, m, *d).is_some()).unwrap_or(28);
    NaiveDate::from_ymd_opt(y, m, date.day().min(last)).expect("valid date")
}

/// The period that contains `at` (start inclusive, end exclusive), at local midnights.
/// At a boundary, the period that starts there.
pub fn period_bounds(length: PeriodLength, at: DateTime<Utc>, tz: Tz) -> (DateTime<Utc>, DateTime<Utc>) {
    let today = at.with_timezone(&tz).date_naive();
    let (start, end) = match length {
        PeriodLength::Monthly { start_day } => {
            let day = u32::from(start_day.clamp(1, 28));
            let this = NaiveDate::from_ymd_opt(today.year(), today.month(), day).expect("day ≤ 28");
            let start = if today >= this { this } else { prev_month(this) };
            (start, add_months(start, 1))
        }
        PeriodLength::Weekly { start_weekday } => {
            let wd = i64::from(today.weekday().number_from_monday());
            let back = (wd - i64::from(start_weekday.clamp(1, 7))).rem_euclid(7);
            let start = today - Duration::days(back);
            (start, start + Duration::days(7))
        }
        PeriodLength::CustomDays { days } => {
            // Fixed anchor (a Monday), so periods tile the calendar the same way everywhere.
            let anchor = NaiveDate::from_ymd_opt(2026, 1, 5).expect("valid anchor");
            let len = i64::from(days.max(1));
            let n = (today - anchor).num_days().div_euclid(len);
            let start = anchor + Duration::days(n * len);
            (start, start + Duration::days(len))
        }
    };
    (local_midnight(start, tz), local_midnight(end, tz))
}

fn prev_month(date: NaiveDate) -> NaiveDate {
    let total = date.year() * 12 + date.month0() as i32 - 1;
    NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, date.day()).expect("day ≤ 28")
}

/// Suggested goal: what the family did last period (rounded to 10), otherwise 60 XP per
/// member; scaled when the period is shorter (the first one starts on the day the household
/// was created).
pub fn suggest_target(previous_total: Option<Points>, members: usize, period_share: f64) -> Points {
    let base = previous_total.filter(|t| *t > 0).unwrap_or(60 * members.max(1) as Points) as f64;
    let scaled = (base * period_share.clamp(0.0, 1.0) / 10.0).round() as Points * 10;
    scaled.max(20)
}

/// Krümel's boost from overdue tasks: `(overdue seconds, xp)` per task.
pub fn overdue_boost(spans: &[(i64, Points)], boost_percent: u32) -> f64 {
    spans.iter().map(|(secs, xp)| (*secs).max(0) as f64 / 86_400.0 * *xp as f64 * f64::from(boost_percent) / 100.0).sum()
}

/// Where Krümel is now: even walk from 0 at `start` to `target` at `end`, plus the boost.
pub fn krumel_position(target: Points, start: DateTime<Utc>, end: DateTime<Utc>, now: DateTime<Utc>, boost: f64) -> Points {
    let total = (end - start).num_seconds().max(1) as f64;
    let elapsed = (now - start).num_seconds().clamp(0, total as i64) as f64;
    let even = target as f64 * elapsed / total;
    ((even + boost).floor() as Points).clamp(0, target)
}

/// Decides the race once somebody arrives. Evaluated whenever the family earns XP (the only
/// moment it can arrive) and whenever Krümel's position is looked at.
pub fn decide(family: Points, krumel: Points, target: Points) -> Option<RaceStatus> {
    if family >= target {
        Some(if krumel < target { RaceStatus::Won } else { RaceStatus::Lost })
    } else if krumel >= target {
        Some(RaceStatus::Lost)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Berlin;

    fn berlin(y: i32, m: u32, d: u32, h: u32) -> DateTime<Utc> {
        Berlin.with_ymd_and_hms(y, m, d, h, 0, 0).unwrap().with_timezone(&Utc)
    }

    #[test]
    fn monthly_periods_follow_the_start_day() {
        let (s, e) = period_bounds(PeriodLength::Monthly { start_day: 1 }, berlin(2026, 11, 22, 15), Berlin);
        assert_eq!((s, e), (berlin(2026, 11, 1, 0), berlin(2026, 12, 1, 0)));
        // Before the start day: the period began last month (across the year).
        let (s, e) = period_bounds(PeriodLength::Monthly { start_day: 15 }, berlin(2027, 1, 3, 9), Berlin);
        assert_eq!((s, e), (berlin(2026, 12, 15, 0), berlin(2027, 1, 15, 0)));
        // At the boundary: the new period.
        let (s, _) = period_bounds(PeriodLength::Monthly { start_day: 1 }, berlin(2026, 12, 1, 0), Berlin);
        assert_eq!(s, berlin(2026, 12, 1, 0));
    }

    #[test]
    fn weekly_and_custom_periods() {
        // 22.11.2026 is a Sunday.
        let (s, e) = period_bounds(PeriodLength::Weekly { start_weekday: 1 }, berlin(2026, 11, 22, 20), Berlin);
        assert_eq!((s, e), (berlin(2026, 11, 16, 0), berlin(2026, 11, 23, 0)));
        let (s, e) = period_bounds(PeriodLength::CustomDays { days: 14 }, berlin(2026, 1, 20, 8), Berlin);
        assert_eq!((s, e), (berlin(2026, 1, 19, 0), berlin(2026, 2, 2, 0)));
    }

    #[test]
    fn krumel_walks_evenly_and_overdue_tasks_push_him() {
        let (s, e) = (berlin(2026, 11, 1, 0), berlin(2026, 12, 1, 0));
        let mid = s + (e - s) / 2;
        assert_eq!(krumel_position(300, s, e, s, 0.0), 0);
        assert_eq!(krumel_position(300, s, e, mid, 0.0), 150);
        assert_eq!(krumel_position(300, s, e, e + Duration::days(2), 0.0), 300, "arrives at the end, not beyond");
        // A 20-XP task overdue for 3 days at 25 %: +15.
        let boost = overdue_boost(&[(3 * 86_400, 20)], 25);
        assert!((boost - 15.0).abs() < 1e-9);
        assert_eq!(krumel_position(300, s, e, mid, boost), 165);
    }

    #[test]
    fn the_first_to_arrive_decides() {
        assert_eq!(decide(299, 200, 300), None);
        assert_eq!(decide(300, 200, 300), Some(RaceStatus::Won));
        assert_eq!(decide(310, 300, 300), Some(RaceStatus::Lost), "Krümel was already there");
        assert_eq!(decide(120, 300, 300), Some(RaceStatus::Lost));
    }

    #[test]
    fn suggested_targets() {
        assert_eq!(suggest_target(Some(287), 3, 1.0), 290);
        assert_eq!(suggest_target(None, 3, 1.0), 180);
        assert_eq!(suggest_target(None, 3, 0.3), 50);
        assert_eq!(suggest_target(Some(10), 1, 0.1), 20, "never below 20");
        assert_eq!(RaceStatus::parse(RaceStatus::Won.as_str()), RaceStatus::Won);
    }
}
