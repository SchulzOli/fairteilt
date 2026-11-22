//! Rhythm of a task template (KONZEPT §7.1): once, "every X days after the last time",
//! weekly on fixed weekdays, or monthly on a day.
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
}

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
        }
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
        };
        Some(Cycle { start: done_at, due })
    }
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
