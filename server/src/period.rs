//! Periods, family goal and settlement as a service (M4, KONZEPT §5, §6). Used by the API,
//! by task completion (the goal can be reached) and by the scheduler (closing, auto-approval).
//!
//! Flow: every household has one running period. At its end the job `close_period` computes
//! the settlement as a draft, decides the race against Krümel and opens the next period.
//! An admin approves the draft (or `approve_settlement` does after `auto_approve_after_hours`):
//! bookings and catch-up tasks are created.

use chrono::{DateTime, Duration, Utc};
use punkte_core::{
    DeficitAction, GoalPolicy, LedgerKind, MemberPeriodInput, PeriodPolicy, RaceStatus, decide_race, krumel_position,
    overdue_boost, period_bounds, suggest_target,
};
use punkte_core::settlement::{pick_penalty_tasks, settle};
use serde::Serialize;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::api::tasks::{Household, household};
use crate::inbox::{self, Category, Note};
use crate::error::{AppError, AppResult};
use crate::AppState;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PeriodRow {
    pub id: Uuid,
    pub household_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub status: String,
}

async fn period_policy(db: &PgPool, household_id: Uuid) -> AppResult<(PeriodPolicy, GoalPolicy)> {
    let (p, g): (serde_json::Value, serde_json::Value) =
        sqlx::query_as("SELECT period_policy, goal_policy FROM households WHERE id = $1").bind(household_id).fetch_one(db).await?;
    Ok((serde_json::from_value(p).map_err(anyhow::Error::from)?, serde_json::from_value(g).map_err(anyhow::Error::from)?))
}

/// The running period of the household; opens one (with its family goal and closing job)
/// when there is none.
pub async fn ensure_period(db: &PgPool, household_id: Uuid) -> AppResult<PeriodRow> {
    let running = sqlx::query_as::<_, PeriodRow>(
        "SELECT id, household_id, starts_at, ends_at, status FROM periods WHERE household_id = $1 AND status = 'running'",
    )
    .bind(household_id)
    .fetch_optional(db)
    .await?;
    if let Some(p) = running {
        return Ok(p);
    }
    let hh = household(db, household_id).await?;
    let (policy, goal) = period_policy(db, household_id).await?;
    let mut tx = db.begin().await?;
    // Serialize: only one request opens the next period.
    sqlx::query("SELECT id FROM households WHERE id = $1 FOR UPDATE").bind(household_id).fetch_one(&mut *tx).await?;
    if let Some(p) = sqlx::query_as::<_, PeriodRow>(
        "SELECT id, household_id, starts_at, ends_at, status FROM periods WHERE household_id = $1 AND status = 'running'",
    )
    .bind(household_id)
    .fetch_optional(&mut *tx)
    .await?
    {
        return Ok(p);
    }
    let last: Option<(Uuid, DateTime<Utc>, DateTime<Utc>)> =
        sqlx::query_as("SELECT id, starts_at, ends_at FROM periods WHERE household_id = $1 ORDER BY ends_at DESC LIMIT 1")
            .bind(household_id)
            .fetch_optional(&mut *tx)
            .await?;
    let now = Utc::now();
    // Periods follow each other without gaps; the first one starts today at 00:00 (household
    // time). Not "now": the period opens lazily, often right after the first XP was booked, and
    // those bookings carry the database's clock.
    let start = last.map(|l| l.2).unwrap_or_else(|| punkte_core::goal::local_midnight(now.with_timezone(&hh.tz).date_naive(), hh.tz));
    let (full_start, end) = period_bounds(policy.length, start, hh.tz);
    let share = (end - start).num_seconds() as f64 / (end - full_start).num_seconds().max(1) as f64;

    let target = match goal.target {
        Some(t) => t,
        None => {
            let previous = match last {
                Some((_, s, e)) => Some(family_xp(&mut *tx, household_id, s, e).await?),
                None => None,
            };
            let members: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM memberships WHERE household_id = $1 AND active").bind(household_id).fetch_one(&mut *tx).await?;
            suggest_target(previous, members as usize, share)
        }
    };
    let period = sqlx::query_as::<_, PeriodRow>(
        "INSERT INTO periods (id, household_id, starts_at, ends_at) VALUES ($1, $2, $3, $4)
         RETURNING id, household_id, starts_at, ends_at, status",
    )
    .bind(Uuid::new_v4())
    .bind(household_id)
    .bind(start)
    .bind(end)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("INSERT INTO family_goals (period_id, target, reward, boost_percent) VALUES ($1, $2, $3, $4)")
        .bind(period.id)
        .bind(target)
        .bind(&goal.reward)
        .bind(goal.boost_percent as i32)
        .execute(&mut *tx)
        .await?;
    schedule(&mut tx, "close_period", serde_json::json!({ "period_id": period.id }), end).await?;
    if let Some(hours) = policy.warn_before_end_hours {
        let at = end - Duration::hours(i64::from(hours));
        if at > now {
            schedule(&mut tx, "warn_period", serde_json::json!({ "period_id": period.id }), at).await?;
        }
    }
    tx.commit().await?;
    Ok(period)
}

async fn schedule(tx: &mut Transaction<'_, Postgres>, kind: &str, payload: serde_json::Value, at: DateTime<Utc>) -> AppResult<()> {
    sqlx::query("INSERT INTO jobs (id, kind, payload, run_at) VALUES ($1, $2, $3, $4)")
        .bind(Uuid::new_v4())
        .bind(kind)
        .bind(payload)
        .bind(at)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// XP the whole family earned in `[start, end)` (ledger, so corrections count too).
pub async fn family_xp<'e, E: sqlx::PgExecutor<'e>>(db: E, household_id: Uuid, start: DateTime<Utc>, end: DateTime<Utc>) -> AppResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(SUM(contribution_delta), 0)::BIGINT FROM ledger_entries WHERE household_id = $1 AND created_at >= $2 AND created_at < $3",
    )
    .bind(household_id)
    .bind(start)
    .bind(end)
    .fetch_one(db)
    .await?)
}

/// Where Krümel is: even walk plus the push of every overdue task (share of its XP per day).
async fn krumel_now(db: &PgPool, period: &PeriodRow, target: i64, boost_percent: i32, now: DateTime<Utc>) -> AppResult<i64> {
    let spans: Vec<(f64, i64)> = sqlx::query_as(
        "SELECT EXTRACT(EPOCH FROM LEAST(COALESCE(completed_at, $4), $4, $3) - GREATEST(due_at, $2))::FLOAT8, points_default
         FROM tasks
         WHERE household_id = $1 AND status <> 'cancelled' AND due_at IS NOT NULL AND due_at < LEAST($4, $3)
           AND (completed_at IS NULL OR completed_at > GREATEST(due_at, $2))",
    )
    .bind(period.household_id)
    .bind(period.starts_at)
    .bind(period.ends_at)
    .bind(now)
    .fetch_all(db)
    .await?;
    let spans: Vec<(i64, i64)> = spans.into_iter().map(|(s, xp)| (s as i64, xp)).collect();
    let boost = overdue_boost(&spans, boost_percent.max(0) as u32);
    Ok(krumel_position(target, period.starts_at, period.ends_at, now, boost))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Debug, Clone, Serialize)]
pub struct GoalDto {
    pub target: i64,
    pub reward: String,
    pub family_xp: i64,
    pub krumel_xp: i64,
    pub status: RaceStatus,
    pub decided_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct GoalRow {
    target: i64,
    reward: String,
    boost_percent: i32,
    status: String,
    decided_at: Option<DateTime<Utc>>,
}

/// The goal of the running period; decides the race if someone has arrived.
/// Returns the state and the decision made just now (for the live event).
pub async fn check_goal(db: &PgPool, household_id: Uuid) -> AppResult<(PeriodRow, GoalDto, Option<RaceStatus>)> {
    let period = ensure_period(db, household_id).await?;
    let g = sqlx::query_as::<_, GoalRow>("SELECT target, reward, boost_percent, status, decided_at FROM family_goals WHERE period_id = $1")
        .bind(period.id)
        .fetch_one(db)
        .await?;
    let now = Utc::now().min(period.ends_at);
    let family = family_xp(db, household_id, period.starts_at, period.ends_at).await?;
    let krumel = krumel_now(db, &period, g.target, g.boost_percent, now).await?;
    let mut status = RaceStatus::parse(&g.status);
    let mut decided_at = g.decided_at;
    let mut decided_now = None;
    if status == RaceStatus::Running
        && let Some(result) = decide_race(family, krumel, g.target)
    {
        let changed: Option<DateTime<Utc>> = sqlx::query_scalar(
            "UPDATE family_goals SET status = $2, decided_at = now() WHERE period_id = $1 AND status = 'running' RETURNING decided_at",
        )
        .bind(period.id)
        .bind(result.as_str())
        .fetch_optional(db)
        .await?;
        if let Some(at) = changed {
            status = result;
            decided_at = Some(at);
            decided_now = Some(result);
            let mut conn = db.acquire().await?;
            let (title, body) = match result {
                RaceStatus::Won => ("Geschafft! Ihr habt Krümel geschlagen".to_string(), format!("Familienbelohnung: {}", g.reward)),
                _ => ("Krümel war diesmal schneller".to_string(), "Nächste Runde, neues Glück.".to_string()),
            };
            for m in inbox::members(&mut conn, household_id, false, None).await? {
                inbox::add(&mut conn, Note::new(m, "goal", Category::Period, title.clone()).body(body.clone())).await?;
            }
        }
    }
    let goal = GoalDto { target: g.target, reward: g.reward, family_xp: family, krumel_xp: krumel, status, decided_at };
    Ok((period, goal, decided_now))
}

/// After XP were credited: maybe the family just reached the goal. Tells everyone.
pub async fn after_credit(state: &AppState, household_id: Uuid) {
    match check_goal(&state.db, household_id).await {
        Ok((_, goal, Some(result))) => {
            let kind = if result == RaceStatus::Won { "goal_won" } else { "goal_lost" };
            state.live.notify_json(household_id, serde_json::json!({ "type": kind, "reward": goal.reward, "target": goal.target }));
        }
        Ok(_) => {}
        Err(err) => tracing::warn!(%err, %household_id, "checking the family goal failed"),
    }
}

/// Start of the running period (XP "this period", transfer limits, least loaded).
pub async fn period_start(db: &PgPool, household_id: Uuid) -> AppResult<DateTime<Utc>> {
    Ok(ensure_period(db, household_id).await?.starts_at)
}

// ---------------------------------------------------------------- closing

/// Share of `[start, end)` a member was not absent, from the `absences` table.
async fn availability(db: &PgPool, member: Uuid, period: &PeriodRow, hh: &Household) -> AppResult<f64> {
    let absences: Vec<(chrono::NaiveDate, chrono::NaiveDate)> =
        sqlx::query_as("SELECT starts_on, ends_on FROM absences WHERE member_id = $1").bind(member).fetch_all(db).await?;
    if absences.is_empty() {
        return Ok(1.0);
    }
    let first = period.starts_at.with_timezone(&hh.tz).date_naive();
    let last = (period.ends_at - Duration::seconds(1)).with_timezone(&hh.tz).date_naive();
    let days = (last - first).num_days() + 1;
    let absent: i64 = (0..days)
        .map(|i| first + Duration::days(i))
        .filter(|d| absences.iter().any(|(s, e)| s <= d && d <= e))
        .count() as i64;
    Ok(1.0 - absent as f64 / days.max(1) as f64)
}

/// Closes a period: draft settlement, race decided, next period opened. Idempotent.
pub async fn close_period(db: &PgPool, period_id: Uuid) -> AppResult<Option<Uuid>> {
    let period = sqlx::query_as::<_, PeriodRow>("SELECT id, household_id, starts_at, ends_at, status FROM periods WHERE id = $1")
        .bind(period_id)
        .fetch_optional(db)
        .await?
        .ok_or(AppError::NotFound)?;
    if period.status != "running" {
        return Ok(None);
    }
    let hid = period.household_id;
    let hh = household(db, hid).await?;
    let (policy, _) = period_policy(db, hid).await?;
    let inputs = period_inputs(db, &period, &hh).await?;
    let result = settle(&policy, &inputs);
    let family: i64 = result.total_contribution;
    close_tx(db, &period, &policy, &result, family).await
}

/// Per-member input for the settlement of `period` (XP so far, carry, availability).
async fn period_inputs(db: &PgPool, period: &PeriodRow, hh: &Household) -> AppResult<Vec<MemberPeriodInput>> {
    let hid = period.household_id;
    #[derive(sqlx::FromRow)]
    struct Member {
        id: Uuid,
        weight: f64,
        contribution: i64,
        carry_in: i64,
    }
    let members = sqlx::query_as::<_, Member>(
        "SELECT m.id, m.weight,
                COALESCE((SELECT SUM(l.contribution_delta) FROM ledger_entries l
                          WHERE l.member_id = m.id AND l.created_at >= $2 AND l.created_at < $3), 0)::BIGINT AS contribution,
                COALESCE((SELECT sl.carry_next FROM settlement_lines sl JOIN settlements s ON s.id = sl.settlement_id
                          JOIN periods p ON p.id = s.period_id
                          WHERE sl.member_id = m.id AND p.household_id = $1 AND p.id <> $4
                          ORDER BY p.ends_at DESC LIMIT 1), 0)::BIGINT AS carry_in
         FROM memberships m WHERE m.household_id = $1 AND m.active ORDER BY m.seq",
    )
    .bind(hid)
    .bind(period.starts_at)
    .bind(period.ends_at)
    .bind(period.id)
    .fetch_all(db)
    .await?;
    let mut inputs = Vec::with_capacity(members.len());
    for m in &members {
        inputs.push(MemberPeriodInput {
            member_id: m.id,
            weight: m.weight,
            availability: availability(db, m.id, period, hh).await?,
            contribution: m.contribution,
            carry_in: m.carry_in,
            quota: None,
        });
    }
    Ok(inputs)
}

async fn close_tx(
    db: &PgPool,
    period: &PeriodRow,
    policy: &PeriodPolicy,
    result: &punkte_core::Settlement,
    family: i64,
) -> AppResult<Option<Uuid>> {
    let hid = period.household_id;
    let mut tx = db.begin().await?;
    let still_running: Option<String> =
        sqlx::query_scalar("SELECT status FROM periods WHERE id = $1 FOR UPDATE").bind(period.id).fetch_optional(&mut *tx).await?;
    if still_running.as_deref() != Some("running") {
        return Ok(None);
    }
    let settlement_id = Uuid::new_v4();
    let auto_at = policy.auto_approve_after_hours.map(|h| Utc::now() + Duration::hours(i64::from(h)));
    sqlx::query("INSERT INTO settlements (id, period_id, policy, auto_approve_at) VALUES ($1, $2, $3, $4)")
        .bind(settlement_id)
        .bind(period.id)
        .bind(serde_json::to_value(policy).map_err(anyhow::Error::from)?)
        .bind(auto_at)
        .execute(&mut *tx)
        .await?;
    for line in &result.lines {
        sqlx::query(
            "INSERT INTO settlement_lines (settlement_id, member_id, target, contribution, deviation, outcome, balance_delta,
                                           penalty_points_due, carry_next)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(settlement_id)
        .bind(line.member_id)
        .bind(line.target)
        .bind(line.contribution)
        .bind(line.deviation)
        .bind(serde_json::to_value(line.outcome).map_err(anyhow::Error::from)?.as_str().unwrap_or("within_tolerance").to_string())
        .bind(line.balance_delta)
        .bind(line.penalty_points_due)
        .bind(line.carry_next)
        .execute(&mut *tx)
        .await?;
    }
    // The race ends with the period: Krümel has arrived.
    sqlx::query(
        "UPDATE family_goals SET status = CASE WHEN $2 >= target THEN 'won' ELSE 'lost' END, decided_at = now()
         WHERE period_id = $1 AND status = 'running'",
    )
    .bind(period.id)
    .bind(family)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE periods SET status = 'settling' WHERE id = $1").bind(period.id).execute(&mut *tx).await?;
    if let Some(at) = auto_at {
        schedule(&mut tx, "approve_settlement", serde_json::json!({ "settlement_id": settlement_id }), at).await?;
    }
    for admin in inbox::members(&mut tx, hid, true, None).await? {
        let body = if auto_at.is_some() { "Ansehen und freigeben, sonst geschieht das automatisch." } else { "Ansehen und freigeben." };
        inbox::add(&mut tx, Note::new(admin, "settlement_draft", Category::Period, "Der Abschluss ist fertig").body(body).about("settlement", settlement_id))
            .await?;
    }
    tx.commit().await?;
    ensure_period(db, hid).await?;
    Ok(Some(settlement_id))
}

/// Books the approved settlement: bonuses, deductions (never below 0 unless allowed),
/// zero-sum payments and catch-up tasks. Idempotent. `by` = None for the automatic approval.
pub async fn approve_settlement(db: &PgPool, settlement_id: Uuid, by: Option<Uuid>) -> AppResult<bool> {
    let mut tx = db.begin().await?;
    let row: Option<(Uuid, String, serde_json::Value)> =
        sqlx::query_as("SELECT period_id, status, policy FROM settlements WHERE id = $1 FOR UPDATE").bind(settlement_id).fetch_optional(&mut *tx).await?;
    let (period_id, status, policy) = row.ok_or(AppError::NotFound)?;
    if status == "approved" {
        return Ok(false);
    }
    let policy: PeriodPolicy = serde_json::from_value(policy).map_err(anyhow::Error::from)?;
    let (hid, ends_at): (Uuid, DateTime<Utc>) =
        sqlx::query_as("SELECT household_id, ends_at FROM periods WHERE id = $1").bind(period_id).fetch_one(&mut *tx).await?;
    let hh = household(db, hid).await?;
    let lines: Vec<(Uuid, i64, i64)> =
        sqlx::query_as("SELECT member_id, balance_delta, penalty_points_due FROM settlement_lines WHERE settlement_id = $1")
            .bind(settlement_id)
            .fetch_all(&mut *tx)
            .await?;
    let zero_sum = policy.deficit_action == DeficitAction::ZeroSum;
    let note = format!("Abschluss bis {}", ends_at.with_timezone(&hh.tz).format("%d.%m.%Y"));
    for (member, delta, penalty) in lines {
        if delta > 0 {
            let kind = if zero_sum { LedgerKind::SettlementZeroSum } else { LedgerKind::SettlementBonus };
            book(&mut tx, hid, member, kind, delta, settlement_id, &note, by).await?;
        } else if delta < 0 {
            sqlx::query("SELECT id FROM memberships WHERE id = $1 FOR UPDATE").bind(member).fetch_one(&mut *tx).await?;
            let have: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(balance_delta), 0)::BIGINT FROM ledger_entries WHERE member_id = $1")
                .bind(member)
                .fetch_one(&mut *tx)
                .await?;
            let take = if hh.policy.allow_negative_balance { -delta } else { (-delta).min(have.max(0)) };
            if take > 0 {
                let kind = if zero_sum { LedgerKind::SettlementZeroSum } else { LedgerKind::PenaltyDeduction };
                book(&mut tx, hid, member, kind, -take, settlement_id, &note, by).await?;
            }
        }
        if penalty > 0 {
            catch_up_tasks(&mut tx, hid, member, penalty, settlement_id, &hh).await?;
        }
    }
    for m in inbox::members(&mut tx, hid, false, by).await? {
        inbox::add(&mut tx, Note::new(m, "settlement_approved", Category::Period, "Der Abschluss ist freigegeben").body(note.clone()).about("settlement", settlement_id))
            .await?;
    }
    sqlx::query("UPDATE settlements SET status = 'approved', approved_by = $2, approved_at = now() WHERE id = $1")
        .bind(settlement_id)
        .bind(by)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE periods SET status = 'settled' WHERE id = $1").bind(period_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
async fn book(
    tx: &mut Transaction<'_, Postgres>,
    household: Uuid,
    member: Uuid,
    kind: LedgerKind,
    balance: i64,
    settlement_id: Uuid,
    note: &str,
    by: Option<Uuid>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO ledger_entries (id, household_id, member_id, kind, balance_delta, ref_type, ref_id, note, created_by)
         VALUES ($1, $2, $3, $4, $5, 'settlement', $6, $7, $8)",
    )
    .bind(Uuid::new_v4())
    .bind(household)
    .bind(member)
    .bind(kind.as_str())
    .bind(balance)
    .bind(settlement_id)
    .bind(note)
    .bind(by)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Catch-up tasks worth about `due` XP from the household's catch-up templates (largest first),
/// due in 7 days. Without templates: one open "Aufholaufgabe" to agree on together.
async fn catch_up_tasks(
    tx: &mut Transaction<'_, Postgres>,
    household: Uuid,
    member: Uuid,
    due: i64,
    settlement_id: Uuid,
    hh: &Household,
) -> AppResult<()> {
    let options: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT id, points_default FROM task_templates WHERE household_id = $1 AND kind = 'penalty' AND NOT archived ORDER BY created_at",
    )
    .bind(household)
    .fetch_all(&mut **tx)
    .await?;
    let now = Utc::now();
    let due_at = now + Duration::days(7);
    let picked = pick_penalty_tasks(due, &options, 5);
    if picked.is_empty() {
        let points = due.clamp(hh.policy.task_min, hh.policy.task_max);
        let task_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tasks (id, household_id, title, description, icon, kind, status, assignee_id, due_at, cycle_start,
                                points_min, points_default, points_max, settlement_id)
             VALUES ($1, $2, 'Aufholaufgabe', 'Was genau, besprecht ihr gemeinsam.', 'star', 'penalty', 'claimed', $3, $4, $5,
                     $6, $6, $6, $7)",
        )
        .bind(task_id)
        .bind(household)
        .bind(member)
        .bind(due_at)
        .bind(now)
        .bind(points)
        .bind(settlement_id)
        .execute(&mut **tx)
        .await?;
        inbox::add(tx, Note::new(member, "catch_up", Category::Tasks, "Neue Aufholaufgabe").body("Was genau, besprecht ihr gemeinsam.").about("task", task_id))
            .await?;
        return Ok(());
    }
    for template in picked {
        let task_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO tasks (id, household_id, room_id, title, description, icon, kind, status, assignee_id, due_at, cycle_start,
                                points_min, points_default, points_max, step_order, duration_min, settlement_id)
             SELECT $1, household_id, room_id, title, 'Aufholaufgabe aus dem Abschluss', icon, 'penalty', 'claimed', $3, $4, $5,
                    points_min, points_default, points_max, step_order, duration_min, $6
             FROM task_templates WHERE id = $2",
        )
        .bind(task_id)
        .bind(template)
        .bind(member)
        .bind(due_at)
        .bind(now)
        .bind(settlement_id)
        .execute(&mut **tx)
        .await?;
        sqlx::query(
            "INSERT INTO task_steps (id, task_id, position, title) SELECT gen_random_uuid(), $1, position, title FROM template_steps WHERE template_id = $2",
        )
        .bind(task_id)
        .bind(template)
        .execute(&mut **tx)
        .await?;
        let title: String = sqlx::query_scalar("SELECT title FROM tasks WHERE id = $1").bind(task_id).fetch_one(&mut **tx).await?;
        inbox::add(tx, Note::new(member, "catch_up", Category::Tasks, format!("Aufholaufgabe: „{title}“")).body("Fällig in einer Woche.").about("task", task_id))
            .await?;
    }
    Ok(())
}

/// Warning before the period ends (KONZEPT §5.3): members clearly behind their share hear how
/// many XP they are missing. Nothing happens if the period is no longer running.
pub async fn warn_period(db: &PgPool, period_id: Uuid) -> AppResult<usize> {
    let Some(period) = sqlx::query_as::<_, PeriodRow>("SELECT id, household_id, starts_at, ends_at, status FROM periods WHERE id = $1")
        .bind(period_id)
        .fetch_optional(db)
        .await?
    else {
        return Ok(0);
    };
    if period.status != "running" {
        return Ok(0);
    }
    let hh = household(db, period.household_id).await?;
    let (policy, _) = period_policy(db, period.household_id).await?;
    let result = settle(&policy, &period_inputs(db, &period, &hh).await?);
    let hours = (period.ends_at - Utc::now()).num_hours().max(1);
    let left = if hours >= 36 { format!("Noch {} Tage", (hours + 12) / 24) } else { format!("Noch {hours} Stunden") };
    let mut conn = db.acquire().await?;
    let mut n = 0;
    for line in result.lines.iter().filter(|l| l.outcome == punkte_core::Outcome::Deficit) {
        let missing = -line.deviation;
        inbox::add(
            &mut conn,
            Note::new(line.member_id, "period_warning", Category::Period, format!("{left} bis zum Abschluss"))
                .body(format!("Dir fehlen etwa {missing} XP bis zu deinem Anteil.")),
        )
        .await?;
        n += 1;
    }
    Ok(n)
}
