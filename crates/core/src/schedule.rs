//! Rhythm of a task template (KONZEPT §7.1): once, "every X days after the last time",
//! weekly on fixed weekdays, monthly on a day, or "auf Abruf" (M8: rests until triggered).
//!
//! Every recurring template has exactly one current round (`Cycle`) with a start and a
//! due time. Completing a round creates the next one; its start is the completion time,
//! so the condition bar resets ("wer früher putzt, setzt die Uhr zurück").
//! Fixed dates are due at the end of that day in the household's time zone.

use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Schedule {
    /// One-off task, optional due date set on the task itself.
    Once,
    /// Due `days` after the last completion ("nach Bedarf").
    Interval { days: u32 },
    /// Due on these weekdays (1 = Monday … 7 = Sunday), every `every_weeks` weeks.
    Weekly { weekdays: Vec<u8>, every_weeks: u32 },
    /// Due on this day of the month (clamped to the month's length, e.g. 31 → 30 in April).
    Monthly { day: u8 },
    /// "Auf Abruf" (M8): no round until something triggers it (a shopping list that is no
    /// longer empty, or "Ist wieder nötig"). The round is due `due_after_days` later, or has
    /// no due date at all.
    OnCall { due_after_days: Option<u32> },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ScheduleError {
    #[error("Das Intervall muss zwischen 1 und 365 Tagen liegen.")]
    Interval,
    #[error("Bitte mindestens einen Wochentag wählen.")]
    Weekdays,
    #[error("„Alle X Wochen“ muss zwischen 1 und 8 liegen.")]
    EveryWeeks,
    #[error("Der Tag im Monat muss zwischen 1 und 31 liegen.")]
    MonthDay,
    #[error("„Fällig nach“ muss zwischen 1 und 30 Tagen liegen.")]
    DueAfter,
}

/// Share of a round that counts as "fällig" (condition ≥ 0.75, see `condition.rs`): an urgent
/// trigger starts the round right there.
const URGENT_HOURS: i64 = 6;

/// One round of a recurring task.
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cycle {
    pub start: DateTime<Utc>,
    pub due: DateTime<Utc>,
}

/// End of a local calendar day (23:59:59) as UTC. DST gaps/overlaps take the earliest valid time.
pub fn end_of_day(date: NaiveDate, tz: Tz) -> DateTime<Utc> {
    let local = date.and_time(NaiveTime::from_hms_opt(23, 59, 59).expect("valid time"));
    tz.from_local_datetime(&local)
        .earliest()
        .unwrap_or_else(|| tz.from_utc_datetime(&local))
        .with_timezone(&Utc)
}

fn local_date(t: DateTime<Utc>, tz: Tz) -> NaiveDate {
    t.with_timezone(&tz).date_naive()
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (ny, nm) = if month == 12 { (year + 1, 1) } else { (year, month + 1) };
    NaiveDate::from_ymd_opt(ny, nm, 1).expect("valid date").pred_opt().expect("valid date").day()
}

fn month_day(year: i32, month: u32, day: u8) -> NaiveDate {
    let d = u32::from(day).min(days_in_month(year, month));
    NaiveDate::from_ymd_opt(year, month, d).expect("valid date")
}

impl Schedule {
    pub fn validate(&self) -> Result<(), ScheduleError> {
        match self {
            Schedule::Once => Ok(()),
            Schedule::Interval { days } if (1..=365).contains(days) => Ok(()),
            Schedule::Interval { .. } => Err(ScheduleError::Interval),
            Schedule::Weekly { weekdays, every_weeks } => {
                if weekdays.is_empty() || weekdays.iter().any(|d| !(1..=7).contains(d)) {
                    Err(ScheduleError::Weekdays)
                } else if !(1..=8).contains(every_weeks) {
                    Err(ScheduleError::EveryWeeks)
                } else {
                    Ok(())
                }
            }
            Schedule::Monthly { day } if (1..=31).contains(day) => Ok(()),
            Schedule::Monthly { .. } => Err(ScheduleError::MonthDay),
            Schedule::OnCall { due_after_days: Some(d) } if !(1..=30).contains(d) => Err(ScheduleError::DueAfter),
            Schedule::OnCall { .. } => Ok(()),
        }
    }

    pub fn is_on_call(&self) -> bool {
        matches!(self, Schedule::OnCall { .. })
    }

    /// The round that a trigger opens at `now` (only for "auf Abruf"). Urgent: the round is
    /// "fällig" right away (yellow, not yet overdue) and due a few hours later.
    pub fn triggered_cycle(&self, now: DateTime<Utc>, urgent: bool) -> Option<(DateTime<Utc>, Option<DateTime<Utc>>)> {
        let Schedule::OnCall { due_after_days } = self else { return None };
        if urgent {
            return Some(urgent_cycle(now));
        }
        Some((now, due_after_days.map(|d| now + Duration::days(i64::from(d)))))
    }

    pub fn is_recurring(&self) -> bool {
        !matches!(self, Schedule::Once)
    }

    fn weekday_matches(weekdays: &[u8], date: NaiveDate) -> bool {
        weekdays.contains(&(date.weekday().number_from_monday() as u8))
    }

    /// First matching date on or after `from` (weekly), searching at most two weeks.
    fn next_weekday_on_or_after(weekdays: &[u8], from: NaiveDate) -> NaiveDate {
        (0..14)
            .map(|i| from + Duration::days(i))
            .find(|d| Self::weekday_matches(weekdays, *d))
            .unwrap_or(from)
    }

    /// First monthly date on or after `from`.
    fn next_month_day_on_or_after(day: u8, from: NaiveDate) -> NaiveDate {
        let this = month_day(from.year(), from.month(), day);
        if this >= from {
            this
        } else {
            let (y, m) = if from.month() == 12 { (from.year() + 1, 1) } else { (from.year(), from.month() + 1) };
            month_day(y, m, day)
        }
    }

    /// The first round when a template is created at `now`. `None` for one-off tasks.
    pub fn first_cycle(&self, now: DateTime<Utc>, tz: Tz) -> Option<Cycle> {
        let today = local_date(now, tz);
        let due = match self {
            Schedule::Once => return None,
            Schedule::Interval { days } => now + Duration::days(i64::from(*days)),
            Schedule::Weekly { weekdays, .. } => end_of_day(Self::next_weekday_on_or_after(weekdays, today), tz),
            Schedule::Monthly { day } => end_of_day(Self::next_month_day_on_or_after(*day, today), tz),
            Schedule::OnCall { .. } => return None,
        };
        Some(Cycle { start: now, due })
    }

    /// The round after one that was due at `prev_due` and completed at `done_at`.
    /// Fixed dates: the next occurrence after whichever is later, the old due date or the
    /// completion day. So finishing Monday's chore early on Sunday does not skip Monday's
    /// successor, and finishing late does not create an already overdue round.
    pub fn next_cycle(&self, done_at: DateTime<Utc>, prev_due: DateTime<Utc>, tz: Tz) -> Option<Cycle> {
        let base = local_date(done_at, tz).max(local_date(prev_due, tz));
        let due = match self {
            Schedule::Once => return None,
            Schedule::Interval { days } => done_at + Duration::days(i64::from(*days)),
            Schedule::Weekly { weekdays, every_weeks } => {
                let from = base + Duration::days(1) + Duration::days(7 * (i64::from(*every_weeks) - 1));
                end_of_day(Self::next_weekday_on_or_after(weekdays, from), tz)
            }
            Schedule::Monthly { day } => end_of_day(Self::next_month_day_on_or_after(*day, base + Duration::days(1)), tz),
            // The next round comes only from a new trigger.
            Schedule::OnCall { .. } => return None,
        };
        Some(Cycle { start: done_at, due })
    }

    /// Future rounds for the calendar ("Wiederholungen werden aufgefaltet"), assuming each
    /// round is done on its due date. Starts after the round that is due at `current_due`;
    /// at most `max` rounds, none due after `until`.
    pub fn upcoming(&self, current_due: DateTime<Utc>, until: DateTime<Utc>, tz: Tz, max: usize) -> Vec<Cycle> {
        let mut out = Vec::new();
        let mut due = current_due;
        while out.len() < max {
            let Some(next) = self.next_cycle(due, due, tz) else { break };
            if next.due > until || next.due <= due {
                break;
            }
            due = next.due;
            out.push(next);
        }
        out
    }
}

/// Start and due of a round that is "fällig" at `now`: condition 0.75 (see `condition.rs`).
pub fn urgent_cycle(now: DateTime<Utc>) -> (DateTime<Utc>, Option<DateTime<Utc>>) {
    (now - Duration::hours(3 * URGENT_HOURS), Some(now + Duration::hours(URGENT_HOURS)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Berlin;

    fn utc(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn local(t: DateTime<Utc>) -> String {
        t.with_timezone(&Berlin).format("%a %Y-%m-%d %H:%M").to_string()
    }

    #[test]
    fn upcoming_rounds_for_the_calendar() {
        // Every 7 days: rounds follow each other at the due date.
        let s = Schedule::Interval { days: 7 };
        let due = utc("2026-11-25T10:00:00Z");
        let next = s.upcoming(due, utc("2026-12-31T00:00:00Z"), Berlin, 10);
        assert_eq!(next.iter().map(|c| local(c.due)).collect::<Vec<_>>(), [
            "Wed 2026-12-02 11:00", "Wed 2026-12-09 11:00", "Wed 2026-12-16 11:00", "Wed 2026-12-23 11:00", "Wed 2026-12-30 11:00"
        ]);
        // Tuesdays and Fridays, limited by `max`.
        let w = Schedule::Weekly { weekdays: vec![2, 5], every_weeks: 1 };
        let next = w.upcoming(utc("2026-11-24T22:59:59Z"), utc("2027-01-01T00:00:00Z"), Berlin, 3);
        assert_eq!(next.iter().map(|c| local(c.due)).collect::<Vec<_>>(), ["Fri 2026-11-27 23:59", "Tue 2026-12-01 23:59", "Fri 2026-12-04 23:59"]);
        assert!(Schedule::Once.upcoming(due, utc("2027-01-01T00:00:00Z"), Berlin, 5).is_empty());
    }

    #[test]
    fn on_call_rests_until_triggered() {
        let s = Schedule::OnCall { due_after_days: Some(2) };
        let now = utc("2026-11-24T16:00:00Z");
        assert!(s.validate().is_ok());
        assert!(s.is_on_call() && s.is_recurring());
        assert!(s.first_cycle(now, Berlin).is_none());
        assert!(s.next_cycle(now, now, Berlin).is_none());
        assert!(s.upcoming(now, utc("2027-01-01T00:00:00Z"), Berlin, 5).is_empty());
        assert_eq!(s.triggered_cycle(now, false), Some((now, Some(utc("2026-11-26T16:00:00Z")))));
        // Urgent: due in 6 hours and already "fällig" (condition 0.75).
        let (start, due) = s.triggered_cycle(now, true).unwrap();
        assert_eq!(crate::condition(start, due, now), 0.75);
        assert_eq!(Schedule::OnCall { due_after_days: None }.triggered_cycle(now, false), Some((now, None)));
        assert_eq!(Schedule::OnCall { due_after_days: Some(0) }.validate(), Err(ScheduleError::DueAfter));
        assert!(Schedule::Interval { days: 3 }.triggered_cycle(now, false).is_none());
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, r#"{"type":"on_call","due_after_days":2}"#);
    }

    #[test]
    fn validation() {
        assert!(Schedule::Interval { days: 7 }.validate().is_ok());
        assert_eq!(Schedule::Interval { days: 0 }.validate(), Err(ScheduleError::Interval));
        assert_eq!(Schedule::Weekly { weekdays: vec![], every_weeks: 1 }.validate(), Err(ScheduleError::Weekdays));
        assert_eq!(Schedule::Weekly { weekdays: vec![8], every_weeks: 1 }.validate(), Err(ScheduleError::Weekdays));
        assert_eq!(Schedule::Weekly { weekdays: vec![2], every_weeks: 0 }.validate(), Err(ScheduleError::EveryWeeks));
        assert_eq!(Schedule::Monthly { day: 32 }.validate(), Err(ScheduleError::MonthDay));
    }

    #[test]
    fn interval_resets_on_completion() {
        let s = Schedule::Interval { days: 7 };
        let created = utc("2026-11-22T10:00:00Z");
        let first = s.first_cycle(created, Berlin).unwrap();
        assert_eq!(first.due, utc("2026-11-29T10:00:00Z"));
        // Cleaned early after 4 days: the next round starts now and is due 7 days later.
        let done = utc("2026-11-26T18:00:00Z");
        let next = s.next_cycle(done, first.due, Berlin).unwrap();
        assert_eq!(next, Cycle { start: done, due: utc("2026-12-03T18:00:00Z") });
    }

    #[test]
    fn weekly_tuesday_end_of_day_local() {
        let s = Schedule::Weekly { weekdays: vec![2], every_weeks: 1 };
        // Sunday 22.11.2026 → first due Tuesday 24.11. 23:59 Berlin time.
        let first = s.first_cycle(utc("2026-11-22T10:00:00Z"), Berlin).unwrap();
        assert_eq!(local(first.due), "Tue 2026-11-24 23:59");
        // Done early on Monday: next is the Tuesday after the old due date.
        let next = s.next_cycle(utc("2026-11-23T08:00:00Z"), first.due, Berlin).unwrap();
        assert_eq!(local(next.due), "Tue 2026-12-01 23:59");
        // Done late on Thursday: next Tuesday, not an already overdue round.
        let late = s.next_cycle(utc("2026-11-26T08:00:00Z"), first.due, Berlin).unwrap();
        assert_eq!(local(late.due), "Tue 2026-12-01 23:59");
    }

    #[test]
    fn weekly_today_counts_and_several_days() {
        let s = Schedule::Weekly { weekdays: vec![1, 4], every_weeks: 1 }; // Mon + Thu
        let first = s.first_cycle(utc("2026-11-23T07:00:00Z"), Berlin).unwrap(); // Monday morning
        assert_eq!(local(first.due), "Mon 2026-11-23 23:59");
        let next = s.next_cycle(utc("2026-11-23T19:00:00Z"), first.due, Berlin).unwrap();
        assert_eq!(local(next.due), "Thu 2026-11-26 23:59");
    }

    #[test]
    fn every_two_weeks() {
        let s = Schedule::Weekly { weekdays: vec![6], every_weeks: 2 }; // every other Saturday
        let first = s.first_cycle(utc("2026-11-22T10:00:00Z"), Berlin).unwrap();
        assert_eq!(local(first.due), "Sat 2026-11-28 23:59");
        let next = s.next_cycle(utc("2026-11-28T12:00:00Z"), first.due, Berlin).unwrap();
        assert_eq!(local(next.due), "Sat 2026-12-12 23:59");
    }

    #[test]
    fn monthly_clamps_short_months() {
        let s = Schedule::Monthly { day: 31 };
        let first = s.first_cycle(utc("2027-02-10T10:00:00Z"), Berlin).unwrap();
        assert_eq!(local(first.due), "Sun 2027-02-28 23:59");
        let next = s.next_cycle(utc("2027-02-28T12:00:00Z"), first.due, Berlin).unwrap();
        assert_eq!(local(next.due), "Wed 2027-03-31 23:59");
    }

    #[test]
    fn end_of_day_across_dst_change() {
        // 25.10.2026: clocks go back in Berlin; end of day is still 23:59 local (= 22:59 UTC).
        let e = end_of_day(NaiveDate::from_ymd_opt(2026, 10, 25).unwrap(), Berlin);
        assert_eq!(e, utc("2026-10-25T22:59:59Z"));
    }

    #[test]
    fn once_has_no_cycle_and_json_shape() {
        assert!(Schedule::Once.first_cycle(Utc::now(), Berlin).is_none());
        let json = serde_json::to_string(&Schedule::Weekly { weekdays: vec![2], every_weeks: 1 }).unwrap();
        assert_eq!(json, r#"{"type":"weekly","weekdays":[2],"every_weeks":1}"#);
    }
}
