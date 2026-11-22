//! Task templates, current rounds, actions, steps, approval and bounties (M2/M3, KONZEPT §4, §7).
//!
//! Every template has exactly one current round (a `tasks` row that is not completed or
//! cancelled). Completing it credits the escalated XP to the ledger, pays out bounties and
//! creates the next round for recurring templates. Where the household (or template) asks for
//! approval, `done` submits the task and an adult credits it with `review`.

use std::str::FromStr;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use chrono_tz::Tz;
use punkte_core::{
    Assignment, ConditionState, Cycle, LedgerDelta, PointPolicy, PointRange, Schedule, StepOrder, TaskAction, TaskKind, TaskStatus,
    TemplateApproval, all_done, can_check, condition, condition_state,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::api::households::membership;
use crate::api::points::{Ledger, balance, lock_member};
use crate::api::rooms::{check_icon, planner};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::AppState;

const DEFAULT_CHECKIN_MIN: i32 = 25;
const MAX_STEPS: usize = 20;

// ---------------------------------------------------------------- household context

pub(crate) struct Household {
    pub tz: Tz,
    pub policy: PointPolicy,
}

pub(crate) async fn household(db: &sqlx::PgPool, id: Uuid) -> AppResult<Household> {
    let (tz, policy): (String, serde_json::Value) =
        sqlx::query_as("SELECT timezone, point_policy FROM households WHERE id = $1").bind(id).fetch_one(db).await?;
    Ok(Household {
        tz: Tz::from_str(&tz).unwrap_or(chrono_tz::Europe::Berlin),
        policy: serde_json::from_value(policy).map_err(anyhow::Error::from)?,
    })
}

fn kind_parse(s: &str) -> TaskKind {
    match s {
        "reward" => TaskKind::Reward,
        "penalty" => TaskKind::Penalty,
        _ => TaskKind::Regular,
    }
}

/// Start of the current calendar month in the household's time zone (the period until M4).
pub(crate) fn month_start(tz: Tz, now: DateTime<Utc>) -> DateTime<Utc> {
    let local = now.with_timezone(&tz);
    tz.with_ymd_and_hms(local.year(), local.month(), 1, 0, 0, 0)
        .earliest()
        .map(|t| t.with_timezone(&Utc))
        .unwrap_or(now - Duration::days(31))
}

fn kind_str(k: TaskKind) -> &'static str {
    match k {
        TaskKind::Regular => "regular",
        TaskKind::Reward => "reward",
        TaskKind::Penalty => "penalty",
    }
}

fn status_parse(s: &str) -> TaskStatus {
    match s {
        "claimed" => TaskStatus::Claimed,
        "in_progress" => TaskStatus::InProgress,
        "submitted" => TaskStatus::Submitted,
        "completed" => TaskStatus::Completed,
        "cancelled" => TaskStatus::Cancelled,
        _ => TaskStatus::Open,
    }
}

fn status_str(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Open => "open",
        TaskStatus::Claimed => "claimed",
        TaskStatus::InProgress => "in_progress",
        TaskStatus::Submitted => "submitted",
        TaskStatus::Completed => "completed",
        TaskStatus::Cancelled => "cancelled",
    }
}

// ---------------------------------------------------------------- DTOs

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, sqlx::FromRow)]
pub struct TaskStepDto {
    pub id: Uuid,
    pub position: i32,
    pub title: String,
    pub done_at: Option<DateTime<Utc>>,
    pub done_by: Option<Uuid>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct TaskDto {
    pub id: Uuid,
    pub template_id: Option<Uuid>,
    pub title: String,
    pub icon: Option<String>,
    pub room_id: Option<Uuid>,
    pub kind: TaskKind,
    pub status: TaskStatus,
    pub assignee_id: Option<Uuid>,
    pub cycle_start: DateTime<Utc>,
    pub due_at: Option<DateTime<Utc>>,
    /// Share of the round that has passed: < 0.75 fresh, < 1 due, ≥ 1 overdue.
    pub condition: f64,
    pub condition_state: ConditionState,
    /// XP right now (rises while overdue, up to `points_max`).
    pub xp: i64,
    pub points_min: i64,
    pub points_max: i64,
    pub started_at: Option<DateTime<Utc>>,
    /// When the app should gently ask "Wie läuft's?". Time only counts up.
    pub remind_at: Option<DateTime<Utc>>,
    pub duration_min: Option<i32>,
    pub step_order: StepOrder,
    pub steps: Vec<TaskStepDto>,
    pub completed_at: Option<DateTime<Utc>>,
    pub completed_by: Option<Uuid>,
    pub points_awarded: Option<i64>,
    /// Bonus G currently held on this task (paid to whoever completes it).
    pub bounty: i64,
    pub approved_by: Option<Uuid>,
    /// Message from the reviewer ("Super!" or "Bitte noch den Boden").
    pub review_note: Option<String>,
}

#[derive(sqlx::FromRow)]
struct TaskRow {
    id: Uuid,
    template_id: Option<Uuid>,
    title: String,
    icon: Option<String>,
    room_id: Option<Uuid>,
    kind: String,
    status: String,
    assignee_id: Option<Uuid>,
    cycle_start: DateTime<Utc>,
    due_at: Option<DateTime<Utc>>,
    points_min: i64,
    points_default: i64,
    points_max: i64,
    started_at: Option<DateTime<Utc>>,
    remind_at: Option<DateTime<Utc>>,
    duration_min: Option<i32>,
    step_order: String,
    completed_at: Option<DateTime<Utc>>,
    completed_by: Option<Uuid>,
    points_awarded: Option<i64>,
    bounty: i64,
    approved_by: Option<Uuid>,
    review_note: Option<String>,
    household_id: Uuid,
}

/// Columns of `TaskRow`, as a macro so queries stay literal (static) SQL strings.
macro_rules! task_columns {
    () => {
        "id, template_id, title, icon, room_id, kind, status, assignee_id, cycle_start, due_at,
         points_min, points_default, points_max, started_at, remind_at, duration_min, step_order, completed_at, completed_by, points_awarded,
         (SELECT COALESCE(SUM(b.amount), 0) FROM bounties b WHERE b.task_id = tasks.id AND b.status = 'held')::BIGINT AS bounty,
         approved_by, review_note, household_id"
    };
}

fn range_of(t: &TaskRow) -> PointRange {
    PointRange { min: t.points_min, default: t.points_default, max: t.points_max }
}

fn current_xp(policy: &PointPolicy, t: &TaskRow, now: DateTime<Utc>) -> i64 {
    let age = t.due_at.map(|d| (now - d).num_seconds()).unwrap_or(0);
    policy.current_points(&range_of(t), age)
}

fn to_dto(t: TaskRow, steps: Vec<TaskStepDto>, policy: &PointPolicy, now: DateTime<Utc>) -> TaskDto {
    let c = condition(t.cycle_start, t.due_at, now);
    // Submitted: the proposal waiting for review; completed: what was credited.
    let xp = t.points_awarded.unwrap_or_else(|| current_xp(policy, &t, now));
    TaskDto {
        id: t.id,
        template_id: t.template_id,
        title: t.title,
        icon: t.icon,
        room_id: t.room_id,
        kind: kind_parse(&t.kind),
        status: status_parse(&t.status),
        assignee_id: t.assignee_id,
        cycle_start: t.cycle_start,
        due_at: t.due_at,
        condition: c,
        condition_state: condition_state(c),
        xp,
        points_min: t.points_min,
        points_max: t.points_max,
        started_at: t.started_at,
        remind_at: t.remind_at,
        duration_min: t.duration_min,
        step_order: StepOrder::parse(&t.step_order).unwrap_or_default(),
        steps,
        completed_at: t.completed_at,
        completed_by: t.completed_by,
        points_awarded: t.points_awarded,
        bounty: t.bounty,
        approved_by: t.approved_by,
        review_note: t.review_note,
    }
}

async fn steps_of<'e, E: sqlx::PgExecutor<'e>>(db: E, task_ids: &[Uuid]) -> AppResult<Vec<(Uuid, TaskStepDto)>> {
    #[derive(sqlx::FromRow)]
    struct Row {
        task_id: Uuid,
        id: Uuid,
        position: i32,
        title: String,
        done_at: Option<DateTime<Utc>>,
        done_by: Option<Uuid>,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT task_id, id, position, title, done_at, done_by FROM task_steps WHERE task_id = ANY($1) ORDER BY task_id, position",
    )
    .bind(task_ids)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.task_id, TaskStepDto { id: r.id, position: r.position, title: r.title, done_at: r.done_at, done_by: r.done_by }))
        .collect())
}

async fn load_dto(state: &AppState, task_id: Uuid, policy: &PointPolicy) -> AppResult<TaskDto> {
    let row = sqlx::query_as::<_, TaskRow>(concat!("SELECT ", task_columns!(), " FROM tasks WHERE id = $1"))
        .bind(task_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let steps = steps_of(&state.db, &[task_id]).await?.into_iter().map(|(_, s)| s).collect();
    Ok(to_dto(row, steps, policy, Utc::now()))
}

// ---------------------------------------------------------------- templates

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct CreateTemplateRequest {
    pub title: String,
    pub room_id: Option<Uuid>,
    pub icon: Option<String>,
    pub kind: Option<TaskKind>,
    /// Defaults from the household rules when missing.
    pub points: Option<PointRange>,
    pub schedule: Schedule,
    pub assignment: Assignment,
    pub steps: Vec<String>,
    pub step_order: StepOrder,
    /// Expected duration in minutes; the check-in comes after this time.
    pub duration_min: Option<i32>,
    /// Only for one-off tasks: optional due date.
    pub due_at: Option<DateTime<Utc>>,
    /// Approval for this task; default: the household rule.
    pub approval: Option<TemplateApproval>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct TemplateDto {
    pub id: Uuid,
    pub title: String,
    pub room_id: Option<Uuid>,
    pub icon: Option<String>,
    pub kind: TaskKind,
    pub points: PointRange,
    pub schedule: Schedule,
    pub assignment: Assignment,
    pub step_order: StepOrder,
    pub duration_min: Option<i32>,
    pub steps: Vec<String>,
    pub approval: TemplateApproval,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct CreateTemplateResponse {
    pub template_id: Uuid,
    pub task: TaskDto,
}

fn default_range(policy: &PointPolicy) -> PointRange {
    let d = policy.default_points;
    PointRange { min: (d / 2).max(policy.task_min).min(d), default: d, max: (d * 3 / 2).min(policy.task_max).max(d) }
}

/// XP per member in the current calendar month (household time zone), for "least loaded".
async fn month_load(tx: &mut Transaction<'_, Postgres>, household_id: Uuid, tz: Tz, now: DateTime<Utc>) -> AppResult<Vec<(Uuid, i64)>> {
    let month_start = month_start(tz, now);
    let rows: Vec<(Uuid, i64)> = sqlx::query_as(
        "SELECT completed_by, COALESCE(SUM(points_awarded), 0)::BIGINT FROM tasks
         WHERE household_id = $1 AND status = 'completed' AND completed_at >= $2 AND completed_by IS NOT NULL
         GROUP BY completed_by",
    )
    .bind(household_id)
    .bind(month_start)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows)
}

async fn active_members(tx: &mut Transaction<'_, Postgres>, household_id: Uuid) -> AppResult<Vec<Uuid>> {
    Ok(sqlx::query_scalar("SELECT id FROM memberships WHERE household_id = $1 AND active").bind(household_id).fetch_all(&mut **tx).await?)
}

/// Creates the current round of a template: picks the assignee, copies the steps.
async fn spawn_round(
    tx: &mut Transaction<'_, Postgres>,
    template_id: Uuid,
    cycle_start: DateTime<Utc>,
    due_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    tz: Tz,
) -> AppResult<Uuid> {
    #[derive(sqlx::FromRow)]
    struct T {
        household_id: Uuid,
        title: String,
        icon: Option<String>,
        room_id: Option<Uuid>,
        kind: String,
        points_min: i64,
        points_default: i64,
        points_max: i64,
        assignment: serde_json::Value,
        step_order: String,
        duration_min: Option<i32>,
    }
    let t = sqlx::query_as::<_, T>(
        "SELECT household_id, title, icon, room_id, kind, points_min, points_default, points_max, assignment, step_order, duration_min
         FROM task_templates WHERE id = $1",
    )
    .bind(template_id)
    .fetch_one(&mut **tx)
    .await?;

    let assignment: Assignment = serde_json::from_value(t.assignment).unwrap_or(Assignment::Open);
    let members = active_members(tx, t.household_id).await?;
    let load = if matches!(assignment, Assignment::LeastLoaded { .. }) { month_load(tx, t.household_id, tz, now).await? } else { vec![] };
    let (assignee, next_assignment) = assignment.pick(&load, &|m| members.contains(&m));
    if next_assignment != assignment {
        sqlx::query("UPDATE task_templates SET assignment = $2 WHERE id = $1")
            .bind(template_id)
            .bind(serde_json::to_value(&next_assignment).map_err(anyhow::Error::from)?)
            .execute(&mut **tx)
            .await?;
    }

    let task_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO tasks (id, household_id, template_id, room_id, title, icon, kind, status, assignee_id, due_at, cycle_start,
                            points_min, points_default, points_max, step_order, duration_min)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
    )
    .bind(task_id)
    .bind(t.household_id)
    .bind(template_id)
    .bind(t.room_id)
    .bind(&t.title)
    .bind(&t.icon)
    .bind(&t.kind)
    .bind(if assignee.is_some() { "claimed" } else { "open" })
    .bind(assignee)
    .bind(due_at)
    .bind(cycle_start)
    .bind(t.points_min)
    .bind(t.points_default)
    .bind(t.points_max)
    .bind(&t.step_order)
    .bind(t.duration_min)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO task_steps (id, task_id, position, title)
         SELECT gen_random_uuid(), $1, position, title FROM template_steps WHERE template_id = $2",
    )
    .bind(task_id)
    .bind(template_id)
    .execute(&mut **tx)
    .await?;
    Ok(task_id)
}

pub async fn create_template(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<CreateTemplateRequest>,
) -> AppResult<(StatusCode, Json<CreateTemplateResponse>)> {
    let creator = planner(&state, &user, household_id).await?;
    let hh = household(&state.db, household_id).await?;

    let title = req.title.trim();
    if title.is_empty() || title.chars().count() > 80 {
        return Err(AppError::BadRequest("Der Titel muss 1 bis 80 Zeichen lang sein.".into()));
    }
    req.schedule.validate().map_err(|e| AppError::BadRequest(e.to_string()))?;
    let range = req.points.unwrap_or_else(|| default_range(&hh.policy));
    let range = PointRange::new(range.min, range.default, range.max).map_err(|e| AppError::BadRequest(e.to_string()))?;
    hh.policy.validate_range(&range).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let steps: Vec<String> = req.steps.iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
    if steps.len() > MAX_STEPS || steps.iter().any(|s| s.chars().count() > 80) {
        return Err(AppError::BadRequest(format!("Höchstens {MAX_STEPS} Schritte mit je bis zu 80 Zeichen.")));
    }
    if let Some(d) = req.duration_min
        && !(1..=240).contains(&d)
    {
        return Err(AppError::BadRequest("Die Dauer liegt zwischen 1 und 240 Minuten.".into()));
    }
    let icon = check_icon(&req.icon)?;

    let mut tx = state.db.begin().await?;
    if let Some(room) = req.room_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM rooms WHERE id = $1 AND household_id = $2)")
            .bind(room)
            .bind(household_id)
            .fetch_one(&mut *tx)
            .await?;
        if !ok {
            return Err(AppError::BadRequest("Diesen Raum gibt es im Haushalt nicht.".into()));
        }
    }
    let members = active_members(&mut tx, household_id).await?;
    if req.assignment.members().iter().any(|m| !members.contains(m)) {
        return Err(AppError::BadRequest("Zuweisung an jemanden, der nicht im Haushalt ist.".into()));
    }
    if matches!(&req.assignment, Assignment::Rotation { members, .. } | Assignment::LeastLoaded { members } if members.is_empty()) {
        return Err(AppError::BadRequest("Bitte mindestens eine Person für die Zuweisung wählen.".into()));
    }

    let template_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO task_templates (id, household_id, room_id, title, icon, kind, points_min, points_default, points_max,
                                     schedule, assignment, step_order, duration_min, approval)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
    )
    .bind(template_id)
    .bind(household_id)
    .bind(req.room_id)
    .bind(title)
    .bind(&icon)
    .bind(kind_str(req.kind.unwrap_or(TaskKind::Regular)))
    .bind(range.min)
    .bind(range.default)
    .bind(range.max)
    .bind(serde_json::to_value(&req.schedule).map_err(anyhow::Error::from)?)
    .bind(serde_json::to_value(&req.assignment).map_err(anyhow::Error::from)?)
    .bind(req.step_order.as_str())
    .bind(req.duration_min)
    .bind(req.approval.unwrap_or_default().as_str())
    .execute(&mut *tx)
    .await?;
    for (i, s) in steps.iter().enumerate() {
        sqlx::query("INSERT INTO template_steps (id, template_id, position, title) VALUES ($1, $2, $3, $4)")
            .bind(Uuid::new_v4())
            .bind(template_id)
            .bind(i as i32)
            .bind(s)
            .execute(&mut *tx)
            .await?;
    }
    let now = Utc::now();
    let (start, due) = match req.schedule.first_cycle(now, hh.tz) {
        Some(Cycle { start, due }) => (start, Some(due)),
        None => (now, req.due_at),
    };
    let task_id = spawn_round(&mut tx, template_id, start, due, now, hh.tz).await?;
    record_event(&mut tx, task_id, creator, "created", serde_json::json!({})).await?;
    tx.commit().await?;

    state.live.notify(household_id, r#"{"type":"tasks_changed"}"#);
    let task = load_dto(&state, task_id, &hh.policy).await?;
    Ok((StatusCode::CREATED, Json(CreateTemplateResponse { template_id, task })))
}

pub async fn list_templates(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<Vec<TemplateDto>>> {
    membership(&state, user.user_id, household_id).await?;
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        title: String,
        room_id: Option<Uuid>,
        icon: Option<String>,
        kind: String,
        points_min: i64,
        points_default: i64,
        points_max: i64,
        schedule: serde_json::Value,
        assignment: serde_json::Value,
        step_order: String,
        duration_min: Option<i32>,
        steps: Vec<String>,
        approval: String,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT t.id, t.title, t.room_id, t.icon, t.kind, t.points_min, t.points_default, t.points_max, t.schedule, t.assignment,
                t.step_order, t.duration_min, t.approval,
                COALESCE(ARRAY(SELECT s.title FROM template_steps s WHERE s.template_id = t.id ORDER BY s.position), '{}') AS steps
         FROM task_templates t WHERE t.household_id = $1 AND NOT t.archived ORDER BY t.created_at",
    )
    .bind(household_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| TemplateDto {
                id: r.id,
                title: r.title,
                room_id: r.room_id,
                icon: r.icon,
                kind: kind_parse(&r.kind),
                points: PointRange { min: r.points_min, default: r.points_default, max: r.points_max },
                schedule: serde_json::from_value(r.schedule).unwrap_or(Schedule::Once),
                assignment: serde_json::from_value(r.assignment).unwrap_or(Assignment::Open),
                step_order: StepOrder::parse(&r.step_order).unwrap_or_default(),
                duration_min: r.duration_min,
                steps: r.steps,
                approval: TemplateApproval::parse(&r.approval),
            })
            .collect(),
    ))
}

// ---------------------------------------------------------------- tasks

/// Current rounds of the household (not completed or cancelled), most urgent first.
pub async fn list_tasks(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<Vec<TaskDto>>> {
    membership(&state, user.user_id, household_id).await?;
    let hh = household(&state.db, household_id).await?;
    let rows = sqlx::query_as::<_, TaskRow>(concat!(
        "SELECT ",
        task_columns!(),
        " FROM tasks WHERE household_id = $1 AND status NOT IN ('completed', 'cancelled')"
    ))
    .bind(household_id)
    .fetch_all(&state.db)
    .await?;
    let ids: Vec<Uuid> = rows.iter().map(|r| r.id).collect();
    let mut steps = steps_of(&state.db, &ids).await?;
    let now = Utc::now();
    let mut tasks: Vec<TaskDto> = rows
        .into_iter()
        .map(|r| {
            let id = r.id;
            let own: Vec<TaskStepDto> = steps.extract_if(.., |(tid, _)| *tid == id).map(|(_, s)| s).collect();
            to_dto(r, own, &hh.policy, now)
        })
        .collect();
    tasks.sort_by(|a, b| b.condition.total_cmp(&a.condition));
    Ok(Json(tasks))
}

async fn task_household(state: &AppState, user: &AuthUser, task_id: Uuid) -> AppResult<(Uuid, Uuid, String)> {
    let hid: Uuid = sqlx::query_scalar("SELECT household_id FROM tasks WHERE id = $1")
        .bind(task_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let (mid, role) = membership(state, user.user_id, hid).await?;
    Ok((hid, mid, role))
}

pub async fn get_task(State(state): State<AppState>, user: AuthUser, Path(task_id): Path<Uuid>) -> AppResult<Json<TaskDto>> {
    let (hid, _, _) = task_household(&state, &user, task_id).await?;
    let hh = household(&state.db, hid).await?;
    Ok(Json(load_dto(&state, task_id, &hh.policy).await?))
}

async fn record_event(tx: &mut Transaction<'_, Postgres>, task_id: Uuid, actor: Uuid, kind: &str, payload: serde_json::Value) -> AppResult<()> {
    sqlx::query("INSERT INTO task_events (id, task_id, actor_id, type, payload) VALUES ($1, $2, $3, $4, $5)")
        .bind(Uuid::new_v4())
        .bind(task_id)
        .bind(actor)
        .bind(kind)
        .bind(payload)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskCommand {
    Claim,
    Unclaim,
    Start,
    Pause,
    /// "+10 min": ask again later.
    Snooze,
    Done,
    /// Admins: skip this round (recurring tasks get their next round).
    Cancel,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct TaskActionRequest {
    pub action: TaskCommand,
    /// Snooze length in minutes (default 10).
    pub minutes: Option<i32>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct TaskActionResponse {
    pub task: TaskDto,
    /// XP credited when this action completed the task (directly or by approval).
    pub xp_awarded: Option<i64>,
    /// Bonus G paid out together with the XP.
    pub bounty_paid: Option<i64>,
    /// The task now waits for an adult to confirm it.
    pub submitted: bool,
    /// The next round of a recurring task, if one was created.
    pub next_task_id: Option<Uuid>,
}

#[derive(Default)]
struct Outcome {
    xp_awarded: Option<i64>,
    bounty_paid: Option<i64>,
    submitted: bool,
    next_task_id: Option<Uuid>,
}

impl Outcome {
    fn credited(&self) -> bool {
        self.xp_awarded.is_some()
    }
}

async fn approval_required(tx: &mut Transaction<'_, Postgres>, task: &TaskRow, completer_role: &str, hh: &Household) -> AppResult<bool> {
    let setting: Option<String> = match task.template_id {
        Some(id) => sqlx::query_scalar("SELECT approval FROM task_templates WHERE id = $1").bind(id).fetch_optional(&mut **tx).await?,
        None => None,
    };
    let setting = setting.map(|s| TemplateApproval::parse(&s)).unwrap_or_default();
    Ok(setting.required(hh.policy.approval, completer_role == "child"))
}

/// The person is done: the task goes to review, or it is credited right away.
async fn finish(
    tx: &mut Transaction<'_, Postgres>,
    task: &TaskRow,
    actor: Uuid,
    actor_role: &str,
    hh: &Household,
    now: DateTime<Utc>,
) -> AppResult<Outcome> {
    let xp = current_xp(&hh.policy, task, now);
    let submitted = approval_required(tx, task, actor_role, hh).await?;
    sqlx::query(
        "UPDATE tasks SET status = $5, completed_by = $2, completed_at = $3, points_awarded = $4, review_note = NULL,
                          started_at = NULL, remind_at = NULL, assignee_id = COALESCE(assignee_id, $2), updated_at = $3, version = version + 1
         WHERE id = $1",
    )
    .bind(task.id)
    .bind(actor)
    .bind(now)
    .bind(xp)
    .bind(if submitted { "submitted" } else { "completed" })
    .execute(&mut **tx)
    .await?;
    if submitted {
        record_event(tx, task.id, actor, "submitted", serde_json::json!({ "xp": xp })).await?;
        return Ok(Outcome { submitted: true, ..Outcome::default() });
    }
    record_event(tx, task.id, actor, "completed", serde_json::json!({ "xp": xp })).await?;
    let paid = credit(tx, task, actor, xp, None, actor).await?;
    let next = next_round(tx, task, now, hh).await?;
    Ok(Outcome { xp_awarded: Some(xp), bounty_paid: (paid > 0).then_some(paid), submitted: false, next_task_id: next })
}

/// Books the XP (and G, except for catch-up tasks) and pays out held bounties to `member`.
async fn credit(tx: &mut Transaction<'_, Postgres>, task: &TaskRow, member: Uuid, xp: i64, note: Option<&str>, by: Uuid) -> AppResult<i64> {
    let ledger = Ledger { household: task.household_id, by };
    ledger.book(tx, member, LedgerDelta::task_reward(kind_parse(&task.kind), xp), "task", task.id, note).await?;
    let held: Vec<i64> = sqlx::query_scalar("UPDATE bounties SET status = 'paid' WHERE task_id = $1 AND status = 'held' RETURNING amount")
        .bind(task.id)
        .fetch_all(&mut **tx)
        .await?;
    let paid: i64 = held.iter().sum();
    if paid > 0 {
        ledger.book(tx, member, LedgerDelta::bounty_payout(paid), "task", task.id, None).await?;
    }
    Ok(paid)
}

/// A round that will not be done (skipped): bounties go back to who set them.
async fn refund_bounties(tx: &mut Transaction<'_, Postgres>, task: &TaskRow, by: Uuid) -> AppResult<()> {
    let held: Vec<(Uuid, i64)> =
        sqlx::query_as("UPDATE bounties SET status = 'refunded' WHERE task_id = $1 AND status = 'held' RETURNING pledger_id, amount")
            .bind(task.id)
            .fetch_all(&mut **tx)
            .await?;
    let ledger = Ledger { household: task.household_id, by };
    for (pledger, amount) in held {
        ledger.book(tx, pledger, LedgerDelta::bounty_refund(amount), "task", task.id, None).await?;
    }
    Ok(())
}

async fn next_round(tx: &mut Transaction<'_, Postgres>, task: &TaskRow, done_at: DateTime<Utc>, hh: &Household) -> AppResult<Option<Uuid>> {
    let Some(template_id) = task.template_id else { return Ok(None) };
    let schedule: serde_json::Value =
        sqlx::query_scalar("SELECT schedule FROM task_templates WHERE id = $1 AND NOT archived").bind(template_id).fetch_one(&mut **tx).await?;
    let schedule: Schedule = serde_json::from_value(schedule).unwrap_or(Schedule::Once);
    let prev_due = task.due_at.unwrap_or(done_at);
    match schedule.next_cycle(done_at, prev_due, hh.tz) {
        Some(c) => Ok(Some(spawn_round(tx, template_id, c.start, Some(c.due), Utc::now(), hh.tz).await?)),
        None => Ok(None),
    }
}

async fn lock_task(tx: &mut Transaction<'_, Postgres>, task_id: Uuid) -> AppResult<TaskRow> {
    // Lock first (FOR UPDATE does not combine with the aggregate column), then read.
    let found: Option<Uuid> = sqlx::query_scalar("SELECT id FROM tasks WHERE id = $1 FOR UPDATE").bind(task_id).fetch_optional(&mut **tx).await?;
    found.ok_or(AppError::NotFound)?;
    Ok(sqlx::query_as::<_, TaskRow>(concat!("SELECT ", task_columns!(), " FROM tasks WHERE id = $1")).bind(task_id).fetch_one(&mut **tx).await?)
}

fn notify(state: &AppState, hid: Uuid, outcome: &Outcome) {
    state.live.notify(hid, r#"{"type":"tasks_changed"}"#);
    if outcome.credited() {
        state.live.notify(hid, r#"{"type":"points_changed"}"#);
    }
}

async fn respond(state: &AppState, task_id: Uuid, hh: &Household, o: Outcome) -> AppResult<Json<TaskActionResponse>> {
    Ok(Json(TaskActionResponse {
        task: load_dto(state, task_id, &hh.policy).await?,
        xp_awarded: o.xp_awarded,
        bounty_paid: o.bounty_paid,
        submitted: o.submitted,
        next_task_id: o.next_task_id,
    }))
}

pub async fn action(
    State(state): State<AppState>,
    user: AuthUser,
    Path(task_id): Path<Uuid>,
    Json(req): Json<TaskActionRequest>,
) -> AppResult<Json<TaskActionResponse>> {
    let (hid, me, role) = task_household(&state, &user, task_id).await?;
    let hh = household(&state.db, hid).await?;
    let now = Utc::now();
    let mut tx = state.db.begin().await?;
    let task = lock_task(&mut tx, task_id).await?;
    let status = status_parse(&task.status);
    let invalid = || AppError::Conflict("Das geht in diesem Zustand der Aufgabe nicht.".into());

    let mut outcome = Outcome::default();
    match req.action {
        TaskCommand::Snooze => {
            if status != TaskStatus::InProgress {
                return Err(invalid());
            }
            let minutes = req.minutes.unwrap_or(10).clamp(1, 120);
            sqlx::query("UPDATE tasks SET remind_at = $2, updated_at = now() WHERE id = $1")
                .bind(task_id)
                .bind(now + Duration::minutes(i64::from(minutes)))
                .execute(&mut *tx)
                .await?;
        }
        TaskCommand::Cancel => {
            if role != "admin" {
                return Err(AppError::Forbidden("Nur Admins können eine Runde überspringen.".into()));
            }
            status.apply(TaskAction::Cancel, false).map_err(|_| invalid())?;
            sqlx::query("UPDATE tasks SET status = 'cancelled', started_at = NULL, remind_at = NULL, updated_at = now() WHERE id = $1")
                .bind(task_id)
                .execute(&mut *tx)
                .await?;
            refund_bounties(&mut tx, &task, me).await?;
            outcome.next_task_id = next_round(&mut tx, &task, now, &hh).await?;
        }
        TaskCommand::Done => {
            status.apply(TaskAction::Done, false).map_err(|_| invalid())?;
            let done: Vec<bool> = sqlx::query_scalar("SELECT done_at IS NOT NULL FROM task_steps WHERE task_id = $1 ORDER BY position")
                .bind(task_id)
                .fetch_all(&mut *tx)
                .await?;
            if !all_done(&done) {
                return Err(AppError::Conflict("Erst alle Schritte abhaken, dann ist die Aufgabe erledigt.".into()));
            }
            outcome = finish(&mut tx, &task, me, &role, &hh, now).await?;
        }
        cmd => {
            let action = match cmd {
                TaskCommand::Claim => TaskAction::Claim,
                TaskCommand::Unclaim => TaskAction::Unclaim,
                TaskCommand::Start => TaskAction::Start,
                _ => TaskAction::Pause,
            };
            let next = status.apply(action, false).map_err(|_| invalid())?;
            let (assignee, started, remind) = match action {
                TaskAction::Claim => (Some(me), None, None),
                TaskAction::Unclaim => (None, None, None),
                // Starting takes the task over; time counts up, the check-in comes after the planned duration.
                TaskAction::Start => {
                    let minutes = task.duration_min.unwrap_or(DEFAULT_CHECKIN_MIN);
                    (Some(me), Some(now), Some(now + Duration::minutes(i64::from(minutes))))
                }
                _ => (task.assignee_id, None, None),
            };
            sqlx::query(
                "UPDATE tasks SET status = $2, assignee_id = $3, started_at = $4, remind_at = $5, updated_at = now(), version = version + 1
                 WHERE id = $1",
            )
            .bind(task_id)
            .bind(status_str(next))
            .bind(assignee)
            .bind(started)
            .bind(remind)
            .execute(&mut *tx)
            .await?;
        }
    }
    let event = match req.action {
        TaskCommand::Claim => "claimed",
        TaskCommand::Unclaim => "unclaimed",
        TaskCommand::Start => "started",
        TaskCommand::Pause => "paused",
        TaskCommand::Snooze => "snoozed",
        TaskCommand::Done => "done",
        TaskCommand::Cancel => "cancelled",
    };
    if req.action != TaskCommand::Done {
        record_event(&mut tx, task_id, me, event, serde_json::json!({})).await?;
    }
    tx.commit().await?;
    notify(&state, hid, &outcome);
    if req.action == TaskCommand::Cancel {
        state.live.notify(hid, r#"{"type":"points_changed"}"#);
    }
    respond(&state, task_id, &hh, outcome).await
}

/// Checks one step. Checking a step means working on it: an open task becomes started.
/// The last step finishes the task.
pub async fn check_step(
    State(state): State<AppState>,
    user: AuthUser,
    Path((task_id, step_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Json<TaskActionResponse>> {
    let (hid, me, role) = task_household(&state, &user, task_id).await?;
    let hh = household(&state.db, hid).await?;
    let now = Utc::now();
    let mut tx = state.db.begin().await?;
    let task = lock_task(&mut tx, task_id).await?;
    let status = status_parse(&task.status);
    if status.is_final() || status == TaskStatus::Submitted {
        return Err(AppError::Conflict("Diese Aufgabe ist schon erledigt.".into()));
    }
    let steps: Vec<(Uuid, bool)> = sqlx::query_as("SELECT id, done_at IS NOT NULL FROM task_steps WHERE task_id = $1 ORDER BY position")
        .bind(task_id)
        .fetch_all(&mut *tx)
        .await?;
    let index = steps.iter().position(|(id, _)| *id == step_id).ok_or(AppError::NotFound)?;
    let mut done: Vec<bool> = steps.iter().map(|(_, d)| *d).collect();
    let order = StepOrder::parse(&task.step_order).unwrap_or_default();
    if !can_check(order, &done, index) {
        return Err(AppError::Conflict(if done[index] {
            "Dieser Schritt ist schon erledigt.".into()
        } else {
            "Bitte die Schritte der Reihe nach abhaken.".into()
        }));
    }
    sqlx::query("UPDATE task_steps SET done_at = $2, done_by = $3 WHERE id = $1").bind(step_id).bind(now).bind(me).execute(&mut *tx).await?;
    record_event(&mut tx, task_id, me, "step_done", serde_json::json!({ "step": index })).await?;
    done[index] = true;

    let mut outcome = Outcome::default();
    if all_done(&done) {
        outcome = finish(&mut tx, &task, me, &role, &hh, now).await?;
    } else if status != TaskStatus::InProgress {
        let minutes = task.duration_min.unwrap_or(DEFAULT_CHECKIN_MIN);
        sqlx::query("UPDATE tasks SET status = 'in_progress', assignee_id = $2, started_at = $3, remind_at = $4, updated_at = $3 WHERE id = $1")
            .bind(task_id)
            .bind(me)
            .bind(now)
            .bind(now + Duration::minutes(i64::from(minutes)))
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    notify(&state, hid, &outcome);
    respond(&state, task_id, &hh, outcome).await
}

// ---------------------------------------------------------------- review (M3)

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct ReviewRequest {
    /// true = confirm and credit, false = "bitte nochmal" (back to the person).
    pub approve: bool,
    /// Adjusted XP within the task's min…max (if the household allows it).
    pub points: Option<i64>,
    /// Message to the person who did the task.
    pub note: Option<String>,
}

/// An adult confirms (optionally with adjusted XP) or sends a submitted task back.
/// Nobody reviews their own task (KONZEPT §3).
pub async fn review(
    State(state): State<AppState>,
    user: AuthUser,
    Path(task_id): Path<Uuid>,
    Json(req): Json<ReviewRequest>,
) -> AppResult<Json<TaskActionResponse>> {
    let (hid, me, role) = task_household(&state, &user, task_id).await?;
    if role == "child" {
        return Err(AppError::Forbidden("Bestätigen können nur Erwachsene.".into()));
    }
    let hh = household(&state.db, hid).await?;
    let now = Utc::now();
    let note = req.note.as_deref().map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
    if note.as_ref().is_some_and(|n| n.chars().count() > 300) {
        return Err(AppError::BadRequest("Die Nachricht darf höchstens 300 Zeichen lang sein.".into()));
    }
    let mut tx = state.db.begin().await?;
    let task = lock_task(&mut tx, task_id).await?;
    if status_parse(&task.status) != TaskStatus::Submitted {
        return Err(AppError::Conflict("Diese Aufgabe wartet nicht auf Bestätigung.".into()));
    }
    let completer = task.completed_by.ok_or(AppError::NotFound)?;
    if completer == me {
        return Err(AppError::Forbidden("Die eigene Aufgabe bestätigt bitte jemand anderes.".into()));
    }

    let mut outcome = Outcome::default();
    if req.approve {
        let done_at = task.completed_at.unwrap_or(now);
        let age = task.due_at.map(|d| (done_at - d).num_seconds()).unwrap_or(0);
        let xp = hh.policy.completion_points(&range_of(&task), age, req.points).map_err(|e| AppError::BadRequest(e.to_string()))?;
        sqlx::query(
            "UPDATE tasks SET status = 'completed', points_awarded = $2, approved_by = $3, approved_at = $4, review_note = $5,
                              updated_at = $4, version = version + 1
             WHERE id = $1",
        )
        .bind(task_id)
        .bind(xp)
        .bind(me)
        .bind(now)
        .bind(&note)
        .execute(&mut *tx)
        .await?;
        record_event(&mut tx, task_id, me, "approved", serde_json::json!({ "xp": xp, "note": note })).await?;
        let paid = credit(&mut tx, &task, completer, xp, note.as_deref(), me).await?;
        // The next round starts when the work was done, not when it was confirmed.
        let next = next_round(&mut tx, &task, done_at, &hh).await?;
        outcome = Outcome { xp_awarded: Some(xp), bounty_paid: (paid > 0).then_some(paid), submitted: false, next_task_id: next };
    } else {
        sqlx::query(
            "UPDATE tasks SET status = 'claimed', assignee_id = $2, completed_by = NULL, completed_at = NULL, points_awarded = NULL,
                              review_note = $3, updated_at = now(), version = version + 1
             WHERE id = $1",
        )
        .bind(task_id)
        .bind(completer)
        .bind(&note)
        .execute(&mut *tx)
        .await?;
        record_event(&mut tx, task_id, me, "rejected", serde_json::json!({ "note": note })).await?;
    }
    tx.commit().await?;
    notify(&state, hid, &outcome);
    respond(&state, task_id, &hh, outcome).await
}

// ---------------------------------------------------------------- bounty (M3)

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct BountyRequest {
    pub amount: i64,
}

/// "Wer das Bad putzt, bekommt 20 von mir": the G is held right away (KONZEPT §4.3).
pub async fn pledge(
    State(state): State<AppState>,
    user: AuthUser,
    Path(task_id): Path<Uuid>,
    Json(req): Json<BountyRequest>,
) -> AppResult<Json<TaskDto>> {
    let (hid, me, _) = task_household(&state, &user, task_id).await?;
    let hh = household(&state.db, hid).await?;
    let mut tx = state.db.begin().await?;
    let task = lock_task(&mut tx, task_id).await?;
    let status = status_parse(&task.status);
    if status.is_final() || status == TaskStatus::Submitted {
        return Err(AppError::Conflict("Auf erledigte Aufgaben kann man keinen Bonus mehr setzen.".into()));
    }
    if task.assignee_id == Some(me) {
        return Err(AppError::Forbidden("Auf die eigene Aufgabe kannst du keinen Bonus setzen.".into()));
    }
    lock_member(&mut tx, me).await?;
    let have = balance(&mut tx, me).await?;
    hh.policy.check_bounty(req.amount, have).map_err(|e| AppError::BadRequest(e.to_string()))?;
    sqlx::query("INSERT INTO bounties (id, task_id, pledger_id, amount) VALUES ($1, $2, $3, $4)")
        .bind(Uuid::new_v4())
        .bind(task_id)
        .bind(me)
        .bind(req.amount)
        .execute(&mut *tx)
        .await?;
    Ledger { household: hid, by: me }.book(&mut tx, me, LedgerDelta::bounty_hold(req.amount), "task", task_id, None).await?;
    record_event(&mut tx, task_id, me, "bounty", serde_json::json!({ "amount": req.amount })).await?;
    tx.commit().await?;
    state.live.notify(hid, r#"{"type":"tasks_changed"}"#);
    state.live.notify(hid, r#"{"type":"points_changed"}"#);
    Ok(Json(load_dto(&state, task_id, &hh.policy).await?))
}
