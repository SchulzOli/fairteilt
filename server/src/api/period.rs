//! Period, family goal, settlements and household rules (M4).

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use punkte_core::{DeficitAction, GoalPolicy, PeriodLength, PeriodPolicy, PointPolicy, RaceStatus, SurplusAction};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::households::membership;
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::period::{GoalDto, approve_settlement, check_goal};
use crate::AppState;

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, sqlx::FromRow)]
pub struct SettlementLineDto {
    pub member_id: Uuid,
    pub target: i64,
    pub contribution: i64,
    pub deviation: i64,
    /// `within_tolerance`, `deficit` or `surplus`.
    pub outcome: String,
    pub balance_delta: i64,
    pub penalty_points_due: i64,
    pub carry_next: i64,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct SettlementDto {
    pub id: Uuid,
    pub period_id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    /// `draft` (waiting for approval) or `approved`.
    pub status: String,
    pub auto_approve_at: Option<DateTime<Utc>>,
    pub approved_at: Option<DateTime<Utc>>,
    /// None = approved automatically.
    pub approved_by: Option<Uuid>,
    pub total_xp: i64,
    pub goal: Option<GoalResultDto>,
    pub lines: Vec<SettlementLineDto>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, sqlx::FromRow)]
pub struct GoalResultDto {
    pub target: i64,
    pub reward: String,
    pub status: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct PeriodDto {
    pub id: Uuid,
    pub starts_at: DateTime<Utc>,
    pub ends_at: DateTime<Utc>,
    pub goal: GoalDto,
    /// Last settlements (newest first): a draft waits for approval.
    pub settlements: Vec<SettlementDto>,
}

#[derive(sqlx::FromRow)]
struct SettlementRow {
    id: Uuid,
    period_id: Uuid,
    starts_at: DateTime<Utc>,
    ends_at: DateTime<Utc>,
    status: String,
    auto_approve_at: Option<DateTime<Utc>>,
    approved_at: Option<DateTime<Utc>>,
    approved_by: Option<Uuid>,
}

async fn settlements(state: &AppState, household_id: Uuid, limit: i64) -> AppResult<Vec<SettlementDto>> {
    let rows = sqlx::query_as::<_, SettlementRow>(
        "SELECT s.id, s.period_id, p.starts_at, p.ends_at, s.status, s.auto_approve_at, s.approved_at, s.approved_by
         FROM settlements s JOIN periods p ON p.id = s.period_id
         WHERE p.household_id = $1 ORDER BY p.ends_at DESC LIMIT $2",
    )
    .bind(household_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let lines = sqlx::query_as::<_, SettlementLineDto>(
            "SELECT l.member_id, l.target, l.contribution, l.deviation, l.outcome, l.balance_delta, l.penalty_points_due, l.carry_next
             FROM settlement_lines l JOIN memberships m ON m.id = l.member_id WHERE l.settlement_id = $1 ORDER BY m.seq",
        )
        .bind(r.id)
        .fetch_all(&state.db)
        .await?;
        let goal = sqlx::query_as::<_, GoalResultDto>("SELECT target, reward, status FROM family_goals WHERE period_id = $1")
            .bind(r.period_id)
            .fetch_optional(&state.db)
            .await?;
        out.push(SettlementDto {
            id: r.id,
            period_id: r.period_id,
            starts_at: r.starts_at,
            ends_at: r.ends_at,
            status: r.status,
            auto_approve_at: r.auto_approve_at,
            approved_at: r.approved_at,
            approved_by: r.approved_by,
            total_xp: lines.iter().map(|l| l.contribution).sum(),
            goal,
            lines,
        });
    }
    Ok(out)
}

/// The running period with the race against Krümel and the latest settlements.
pub async fn current(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<PeriodDto>> {
    membership(&state, user.user_id, household_id).await?;
    let (period, goal, decided) = check_goal(&state.db, household_id).await?;
    if let Some(result) = decided {
        // Krümel arrived while nobody was looking.
        let kind = if result == RaceStatus::Won { "goal_won" } else { "goal_lost" };
        state.live.notify_json(household_id, serde_json::json!({ "type": kind, "reward": goal.reward, "target": goal.target }));
    }
    Ok(Json(PeriodDto { id: period.id, starts_at: period.starts_at, ends_at: period.ends_at, goal, settlements: settlements(&state, household_id, 3).await? }))
}

#[derive(Deserialize)]
pub struct ListQuery {
    pub limit: Option<i64>,
}

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<SettlementDto>>> {
    membership(&state, user.user_id, household_id).await?;
    Ok(Json(settlements(&state, household_id, q.limit.unwrap_or(12).clamp(1, 60)).await?))
}

/// An admin approves the draft: bookings and catch-up tasks are created.
pub async fn approve(State(state): State<AppState>, user: AuthUser, Path(settlement_id): Path<Uuid>) -> AppResult<Json<SettlementDto>> {
    let household_id: Uuid = sqlx::query_scalar("SELECT p.household_id FROM settlements s JOIN periods p ON p.id = s.period_id WHERE s.id = $1")
        .bind(settlement_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let (me, role) = membership(&state, user.user_id, household_id).await?;
    if role != "admin" {
        return Err(AppError::Forbidden("Den Abschluss geben Admins frei.".into()));
    }
    if !approve_settlement(&state.db, settlement_id, Some(me)).await? {
        return Err(AppError::Conflict("Dieser Abschluss ist schon freigegeben.".into()));
    }
    state.live.notify_json(household_id, serde_json::json!({ "type": "settlement_approved", "settlement_id": settlement_id, "by": me }));
    let all = settlements(&state, household_id, 12).await?;
    all.into_iter().find(|s| s.id == settlement_id).map(Json).ok_or(AppError::NotFound)
}

// ---------------------------------------------------------------- rules

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize)]
pub struct RulesDto {
    pub point_policy: PointPolicy,
    pub period_policy: PeriodPolicy,
    pub goal_policy: GoalPolicy,
}

pub async fn rules(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<RulesDto>> {
    membership(&state, user.user_id, household_id).await?;
    let (p, q, g): (serde_json::Value, serde_json::Value, serde_json::Value) =
        sqlx::query_as("SELECT point_policy, period_policy, goal_policy FROM households WHERE id = $1").bind(household_id).fetch_one(&state.db).await?;
    Ok(Json(RulesDto {
        point_policy: serde_json::from_value(p).map_err(anyhow::Error::from)?,
        period_policy: serde_json::from_value(q).map_err(anyhow::Error::from)?,
        goal_policy: serde_json::from_value(g).map_err(anyhow::Error::from)?,
    }))
}

fn validate(r: &RulesDto) -> Result<(), String> {
    let p = &r.point_policy;
    if p.task_min < 1 || p.task_min > p.default_points || p.default_points > p.task_max || p.task_max > 1000 {
        return Err("Punkte: es muss 1 ≤ Minimum ≤ Standard ≤ Maximum ≤ 1000 gelten.".into());
    }
    if p.escalation.is_some_and(|e| e.step_points < 0 || e.step_points > 100 || e.step_interval_secs < 3600) {
        return Err("Steigerung: 0 bis 100 XP, höchstens stündlich.".into());
    }
    if p.bounty_max_per_pledge.is_some_and(|m| m < 1) || p.transfer_max_per_period.is_some_and(|m| m < 1) {
        return Err("Grenzen für Bonus und Übertragung müssen mindestens 1 sein.".into());
    }
    let q = &r.period_policy;
    let length_ok = match q.length {
        PeriodLength::Monthly { start_day } => (1..=28).contains(&start_day),
        PeriodLength::Weekly { start_weekday } => (1..=7).contains(&start_weekday),
        PeriodLength::CustomDays { days } => (1..=90).contains(&days),
    };
    if !length_ok {
        return Err("Zeitraum: Monatstag 1 bis 28, Wochentag 1 bis 7 oder 1 bis 90 Tage.".into());
    }
    let pct_ok = |p: u32| p <= 200;
    let actions_ok = match q.deficit_action {
        DeficitAction::BalanceDeduction { percent } | DeficitAction::CarryOver { percent } => pct_ok(percent),
        _ => true,
    } && match q.surplus_action {
        SurplusAction::Bonus { percent } | SurplusAction::CarryOver { percent } => pct_ok(percent),
        SurplusAction::None => true,
    };
    if q.tolerance_percent > 100 || !actions_ok {
        return Err("Toleranz höchstens 100 %, Anteile höchstens 200 %.".into());
    }
    if q.auto_approve_after_hours.is_some_and(|h| h > 24 * 14) {
        return Err("Automatische Freigabe nach höchstens 14 Tagen.".into());
    }
    let g = &r.goal_policy;
    if g.target.is_some_and(|t| !(1..=100_000).contains(&t)) || g.boost_percent > 100 {
        return Err("Familienziel 1 bis 100 000 XP, Schub höchstens 100 %.".into());
    }
    if g.reward.trim().is_empty() || g.reward.chars().count() > 60 {
        return Err("Die Familienbelohnung braucht einen Namen (bis 60 Zeichen).".into());
    }
    Ok(())
}

/// Admins change the household rules. Point rules apply at once; a new period length from the
/// next period on; the goal (target, reward, boost) also for the running race if still open.
pub async fn update_rules(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<RulesDto>,
) -> AppResult<Json<RulesDto>> {
    let (_, role) = membership(&state, user.user_id, household_id).await?;
    if role != "admin" {
        return Err(AppError::Forbidden("Regeln ändern nur Admins.".into()));
    }
    validate(&req).map_err(AppError::BadRequest)?;
    let mut req = req;
    req.goal_policy.reward = req.goal_policy.reward.trim().to_string();
    sqlx::query("UPDATE households SET point_policy = $2, period_policy = $3, goal_policy = $4 WHERE id = $1")
        .bind(household_id)
        .bind(to_json(&req.point_policy)?)
        .bind(to_json(&req.period_policy)?)
        .bind(to_json(&req.goal_policy)?)
        .execute(&state.db)
        .await?;
    let period = crate::period::ensure_period(&state.db, household_id).await?;
    sqlx::query(
        "UPDATE family_goals SET target = COALESCE($2, target), reward = $3, boost_percent = $4 WHERE period_id = $1 AND status = 'running'",
    )
    .bind(period.id)
    .bind(req.goal_policy.target)
    .bind(&req.goal_policy.reward)
    .bind(req.goal_policy.boost_percent as i32)
    .execute(&state.db)
    .await?;
    crate::period::after_credit(&state, household_id).await;
    state.live.notify(household_id, r#"{"type":"points_changed"}"#);
    Ok(Json(req))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct GoalRequest {
    pub target: i64,
    pub reward: String,
    /// Keep this goal for the next periods too (otherwise it is suggested from the last period).
    pub every_period: bool,
}

/// Admins set the goal of the running race (if still open) and what comes after it.
pub async fn update_goal(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<GoalRequest>,
) -> AppResult<Json<GoalDto>> {
    let (_, role) = membership(&state, user.user_id, household_id).await?;
    if role != "admin" {
        return Err(AppError::Forbidden("Das Familienziel legen Admins fest.".into()));
    }
    let reward = req.reward.trim();
    if !(1..=100_000).contains(&req.target) || reward.is_empty() || reward.chars().count() > 60 {
        return Err(AppError::BadRequest("Ziel 1 bis 100 000 XP, Belohnung 1 bis 60 Zeichen.".into()));
    }
    let policy: serde_json::Value = sqlx::query_scalar("SELECT goal_policy FROM households WHERE id = $1").bind(household_id).fetch_one(&state.db).await?;
    let mut policy: GoalPolicy = serde_json::from_value(policy).map_err(anyhow::Error::from)?;
    policy.target = req.every_period.then_some(req.target);
    policy.reward = reward.to_string();
    sqlx::query("UPDATE households SET goal_policy = $2 WHERE id = $1").bind(household_id).bind(to_json(&policy)?).execute(&state.db).await?;
    let period = crate::period::ensure_period(&state.db, household_id).await?;
    sqlx::query("UPDATE family_goals SET target = $2, reward = $3 WHERE period_id = $1 AND status = 'running'")
        .bind(period.id)
        .bind(req.target)
        .bind(reward)
        .execute(&state.db)
        .await?;
    crate::period::after_credit(&state, household_id).await;
    state.live.notify(household_id, r#"{"type":"points_changed"}"#);
    Ok(Json(check_goal(&state.db, household_id).await?.1))
}

fn to_json<T: Serialize>(value: &T) -> AppResult<serde_json::Value> {
    Ok(serde_json::to_value(value).map_err(anyhow::Error::from)?)
}
