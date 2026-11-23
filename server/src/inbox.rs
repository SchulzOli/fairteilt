//! In-app inbox ("Postfach") and the push queue (M5, KONZEPT §8).
//!
//! `add` writes an entry inside the caller's transaction and queues a `send_push` job when the
//! person has a push device and wants pushes of that category. The scheduler delivers the job
//! (respecting quiet hours). Reminders ("bald fällig", "überfällig") come from a sweep job.

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::AppState;
use crate::error::AppResult;
use crate::push::Delivery;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Assigned, bonus on my task, catch-up task, overdue (admins).
    Tasks,
    /// Waiting for my confirmation, confirmed, sent back.
    Reviews,
    /// Corrections of points.
    Points,
    /// Period end, settlement, family goal.
    Period,
    /// My own tasks getting due / overdue (the app also reminds locally).
    Reminders,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Tasks => "tasks",
            Category::Reviews => "reviews",
            Category::Points => "points",
            Category::Period => "period",
            Category::Reminders => "reminders",
        }
    }
}

pub struct Note {
    pub member: Uuid,
    pub kind: &'static str,
    pub category: Category,
    pub title: String,
    pub body: Option<String>,
    /// What to open: `task`, `settlement`, `ledger`.
    pub reference: Option<(&'static str, Uuid)>,
}

impl Note {
    pub fn new(member: Uuid, kind: &'static str, category: Category, title: impl Into<String>) -> Self {
        Self { member, kind, category, title: title.into(), body: None, reference: None }
    }

    pub fn body(mut self, body: impl Into<String>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn body_opt(mut self, body: Option<String>) -> Self {
        self.body = body;
        self
    }

    pub fn about(mut self, ref_type: &'static str, id: Uuid) -> Self {
        self.reference = Some((ref_type, id));
        self
    }
}

/// Which categories go out as push; stored per member (`notification_prefs.channels.push`).
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PushCategories {
    pub tasks: bool,
    pub reviews: bool,
    pub points: bool,
    pub period: bool,
    /// Off by default: the app reminds about your own tasks locally.
    pub reminders: bool,
}

impl Default for PushCategories {
    fn default() -> Self {
        Self { tasks: true, reviews: true, points: true, period: true, reminders: false }
    }
}

impl PushCategories {
    pub fn allows(&self, c: Category) -> bool {
        match c {
            Category::Tasks => self.tasks,
            Category::Reviews => self.reviews,
            Category::Points => self.points,
            Category::Period => self.period,
            Category::Reminders => self.reminders,
        }
    }
}

#[derive(Deserialize, Default)]
struct Channels {
    #[serde(default)]
    push: Option<PushCategories>,
}

pub(crate) async fn push_prefs(conn: &mut PgConnection, member: Uuid) -> AppResult<(PushCategories, Option<NaiveTime>, Option<NaiveTime>)> {
    let row: Option<(serde_json::Value, Option<NaiveTime>, Option<NaiveTime>)> =
        sqlx::query_as("SELECT channels, quiet_from, quiet_to FROM notification_prefs WHERE member_id = $1").bind(member).fetch_optional(&mut *conn).await?;
    Ok(match row {
        Some((channels, from, to)) => {
            let c: Channels = serde_json::from_value(channels).unwrap_or_default();
            (c.push.unwrap_or_default(), from, to)
        }
        None => (PushCategories::default(), None, None),
    })
}

/// Writes an inbox entry (in the caller's transaction) and queues the push if wanted.
pub async fn add(conn: &mut PgConnection, note: Note) -> AppResult<Uuid> {
    let id = Uuid::new_v4();
    let (ref_type, ref_id) = note.reference.map_or((None, None), |(t, i)| (Some(t), Some(i)));
    sqlx::query(
        "INSERT INTO notifications (id, member_id, household_id, type, category, title, body, ref_type, ref_id)
         SELECT $1, m.id, m.household_id, $3, $4, $5, $6, $7, $8 FROM memberships m WHERE m.id = $2",
    )
    .bind(id)
    .bind(note.member)
    .bind(note.kind)
    .bind(note.category.as_str())
    .bind(&note.title)
    .bind(&note.body)
    .bind(ref_type)
    .bind(ref_id)
    .execute(&mut *conn)
    .await?;
    let (prefs, _, _) = push_prefs(conn, note.member).await?;
    if prefs.allows(note.category) {
        let has_device: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM devices d JOIN memberships m ON m.user_id = d.user_id
                            WHERE m.id = $1 AND d.push_token IS NOT NULL)",
        )
        .bind(note.member)
        .fetch_one(&mut *conn)
        .await?;
        if has_device {
            sqlx::query("INSERT INTO jobs (id, kind, payload) VALUES ($1, 'send_push', $2)")
                .bind(Uuid::new_v4())
                .bind(serde_json::json!({ "notification_id": id }))
                .execute(&mut *conn)
                .await?;
        }
    }
    Ok(id)
}

/// Active members of a household: admins only or everyone, without `except`.
pub async fn members(conn: &mut PgConnection, household: Uuid, admins_only: bool, except: Option<Uuid>) -> AppResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar(
        "SELECT id FROM memberships WHERE household_id = $1 AND active AND ($2 = false OR role = 'admin') AND ($3::uuid IS NULL OR id <> $3)
         ORDER BY seq",
    )
    .bind(household)
    .bind(admins_only)
    .bind(except)
    .fetch_all(&mut *conn)
    .await?)
}

/// Adults who can confirm tasks (not children), without `except`.
pub async fn reviewers(conn: &mut PgConnection, household: Uuid, except: Uuid) -> AppResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar("SELECT id FROM memberships WHERE household_id = $1 AND active AND role <> 'child' AND id <> $2 ORDER BY seq")
        .bind(household)
        .bind(except)
        .fetch_all(&mut *conn)
        .await?)
}

pub async fn name(conn: &mut PgConnection, member: Uuid) -> AppResult<String> {
    Ok(sqlx::query_scalar("SELECT u.display_name FROM memberships m JOIN users u ON u.id = m.user_id WHERE m.id = $1")
        .bind(member)
        .fetch_optional(&mut *conn)
        .await?
        .unwrap_or_else(|| "Jemand".into()))
}

/// Inside the quiet window `[from, to)` (may span midnight)?
pub fn is_quiet(now: NaiveTime, from: Option<NaiveTime>, to: Option<NaiveTime>) -> bool {
    match (from, to) {
        (Some(f), Some(t)) if f < t => f <= now && now < t,
        (Some(f), Some(t)) if f > t => now >= f || now < t,
        _ => false,
    }
}

/// Sends the push for one inbox entry. Called by the scheduler (`send_push`).
pub async fn deliver(state: &AppState, notification: Uuid) -> anyhow::Result<()> {
    let Some(fcm) = state.push.as_ref() else {
        tracing::debug!(%notification, "push not configured, inbox only");
        return Ok(());
    };
    let mut conn = state.db.acquire().await?;
    #[derive(sqlx::FromRow)]
    struct Row {
        member_id: Uuid,
        household_id: Option<Uuid>,
        title: String,
        body: Option<String>,
        kind: String,
        ref_type: Option<String>,
        ref_id: Option<Uuid>,
        push_sent_at: Option<DateTime<Utc>>,
        timezone: Option<String>,
    }
    let Some(n) = sqlx::query_as::<_, Row>(
        "SELECT n.member_id, n.household_id, n.title, n.body, n.type AS kind, n.ref_type, n.ref_id, n.push_sent_at, h.timezone
         FROM notifications n LEFT JOIN households h ON h.id = n.household_id WHERE n.id = $1",
    )
    .bind(notification)
    .fetch_optional(&mut *conn)
    .await?
    else {
        return Ok(());
    };
    if n.push_sent_at.is_some() {
        return Ok(());
    }
    // Quiet hours: try again when they end (household time).
    let (_, from, to) = push_prefs(&mut conn, n.member_id).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
    let tz: chrono_tz::Tz = n.timezone.as_deref().and_then(|t| t.parse().ok()).unwrap_or(chrono_tz::Europe::Berlin);
    let local = Utc::now().with_timezone(&tz);
    if is_quiet(local.time(), from, to)
        && let Some(end) = to
    {
        let mut at = tz.from_local_datetime(&local.date_naive().and_time(end)).earliest().unwrap_or(local);
        if at <= local {
            at += Duration::days(1);
        }
        sqlx::query("INSERT INTO jobs (id, kind, payload, run_at) VALUES ($1, 'send_push', $2, $3)")
            .bind(Uuid::new_v4())
            .bind(serde_json::json!({ "notification_id": notification }))
            .bind(at.with_timezone(&Utc))
            .execute(&mut *conn)
            .await?;
        return Ok(());
    }
    let devices: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT d.id, d.push_token FROM devices d JOIN memberships m ON m.user_id = d.user_id
         WHERE m.id = $1 AND d.push_provider = 'fcm' AND d.push_token IS NOT NULL",
    )
    .bind(n.member_id)
    .fetch_all(&mut *conn)
    .await?;
    let data = serde_json::json!({
        "type": n.kind,
        "household_id": n.household_id.map(|h| h.to_string()).unwrap_or_default(),
        "ref_type": n.ref_type.unwrap_or_default(),
        "ref_id": n.ref_id.map(|r| r.to_string()).unwrap_or_default(),
        "notification_id": notification.to_string(),
    });
    let mut failures = Vec::new();
    for (device, token) in &devices {
        match fcm.send(token, &n.title, n.body.as_deref(), data.clone()).await {
            Ok(Delivery::Delivered) => {}
            Ok(Delivery::Unregistered) => {
                sqlx::query("DELETE FROM devices WHERE id = $1").bind(device).execute(&mut *conn).await?;
            }
            Err(err) => failures.push(err),
        }
    }
    if !devices.is_empty() && failures.len() == devices.len() {
        // Nothing went out: let the scheduler retry with backoff.
        anyhow::bail!("push failed: {}", failures[0]);
    }
    sqlx::query("UPDATE notifications SET push_sent_at = now() WHERE id = $1").bind(notification).execute(&mut *conn).await?;
    Ok(())
}

/// Reminders for the person responsible: due soon (75 % of the round), overdue, and after a
/// day overdue the admins. Each only once per round. Returns how many notes were written.
pub async fn sweep_reminders(state: &AppState) -> AppResult<usize> {
    let mut tx = state.db.begin().await?;
    let mut n = 0;
    let due: Vec<(Uuid, Uuid, String, DateTime<Utc>)> = sqlx::query_as(
        "UPDATE tasks SET due_notified_at = now()
         WHERE status IN ('open', 'claimed', 'in_progress') AND assignee_id IS NOT NULL AND due_at IS NOT NULL
           AND due_notified_at IS NULL AND now() < due_at AND now() >= cycle_start + (due_at - cycle_start) * 0.75
         RETURNING id, assignee_id, title, due_at",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (task, member, title, _) in due {
        add(&mut tx, Note::new(member, "due", Category::Reminders, format!("„{title}“ ist bald fällig")).about("task", task)).await?;
        n += 1;
    }
    // Overdue (not older than a week: no flood of old rounds).
    let overdue: Vec<(Uuid, Uuid, String)> = sqlx::query_as(
        "UPDATE tasks SET overdue_notified_at = now()
         WHERE status IN ('open', 'claimed', 'in_progress') AND assignee_id IS NOT NULL AND due_at IS NOT NULL
           AND overdue_notified_at IS NULL AND due_at <= now() AND due_at > now() - interval '7 days'
         RETURNING id, assignee_id, title",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (task, member, title) in overdue {
        add(
            &mut tx,
            Note::new(member, "overdue", Category::Reminders, format!("„{title}“ ist überfällig")).body("Jetzt bringt sie ein paar XP mehr.").about("task", task),
        )
        .await?;
        n += 1;
    }
    // A day overdue: the admins hear about it (KONZEPT §8.1, escalation).
    let late: Vec<(Uuid, Uuid, Option<Uuid>, String)> = sqlx::query_as(
        "UPDATE tasks SET escalated_at = now()
         WHERE status IN ('open', 'claimed', 'in_progress') AND due_at IS NOT NULL AND escalated_at IS NULL
           AND due_at <= now() - interval '1 day' AND due_at > now() - interval '7 days'
         RETURNING id, household_id, assignee_id, title",
    )
    .fetch_all(&mut *tx)
    .await?;
    for (task, household, assignee, title) in late {
        let who = match assignee {
            Some(a) => format!("{} ist dran.", name(&mut tx, a).await?),
            None => "Noch niemand hat sie übernommen.".into(),
        };
        for admin in members(&mut tx, household, true, assignee).await? {
            add(&mut tx, Note::new(admin, "overdue_escalated", Category::Tasks, format!("„{title}“ ist seit einem Tag überfällig")).body(who.clone()).about("task", task))
                .await?;
            n += 1;
        }
    }
    tx.commit().await?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(h: u32, m: u32) -> NaiveTime {
        NaiveTime::from_hms_opt(h, m, 0).unwrap()
    }

    #[test]
    fn quiet_hours_also_over_midnight() {
        assert!(is_quiet(t(23, 0), Some(t(22, 0)), Some(t(7, 0))));
        assert!(is_quiet(t(6, 59), Some(t(22, 0)), Some(t(7, 0))));
        assert!(!is_quiet(t(7, 0), Some(t(22, 0)), Some(t(7, 0))));
        assert!(is_quiet(t(13, 30), Some(t(13, 0)), Some(t(15, 0))));
        assert!(!is_quiet(t(12, 0), Some(t(13, 0)), Some(t(15, 0))));
        assert!(!is_quiet(t(3, 0), None, Some(t(7, 0))));
    }

    #[test]
    fn reminders_are_not_pushed_by_default() {
        let p = PushCategories::default();
        assert!(p.allows(Category::Reviews) && p.allows(Category::Points));
        assert!(!p.allows(Category::Reminders));
    }
}
