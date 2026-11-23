//! Plan (M6, KONZEPT §7.3): schedule and assign tasks, calendar with the coming rounds of
//! recurring tasks, positions on the shared board, saved views.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Duration, Utc};
use punkte_core::{Assignment, Schedule, TaskKind};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::households::membership;
use crate::api::rooms::planner;
use crate::api::tasks::{TaskDto, household, load_dto, record_event, task_household};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::inbox::{self, Category, Note};
use crate::AppState;

// ---------------------------------------------------------------- schedule & assign

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct PlanRequest {
    /// None removes the task from the calendar.
    pub scheduled_start: Option<DateTime<Utc>>,
    /// Defaults to start + planned duration (or 30 minutes).
    pub scheduled_end: Option<DateTime<Utc>>,
}

/// Put a task into the calendar. Planners plan everything, everyone their own tasks.
pub async fn schedule(
    State(state): State<AppState>,
    user: AuthUser,
    Path(task_id): Path<Uuid>,
    Json(req): Json<PlanRequest>,
) -> AppResult<Json<TaskDto>> {
    let (hid, me, role) = task_household(&state, &user, task_id).await?;
    let mut tx = state.db.begin().await?;
    let (status, assignee, duration): (String, Option<Uuid>, Option<i32>) =
        sqlx::query_as("SELECT status, assignee_id, duration_min FROM tasks WHERE id = $1 FOR UPDATE").bind(task_id).fetch_one(&mut *tx).await?;
    if role == "child" && assignee != Some(me) {
        return Err(AppError::Forbidden("Einplanen kannst du nur deine eigenen Aufgaben.".into()));
    }
    if ["completed", "cancelled"].contains(&status.as_str()) {
        return Err(AppError::Conflict("Erledigte Aufgaben plant man nicht mehr ein.".into()));
    }
    let end = match (req.scheduled_start, req.scheduled_end) {
        (None, _) => None,
        (Some(start), Some(end)) => {
            if end <= start || end - start > Duration::hours(24) {
                return Err(AppError::BadRequest("Das Ende liegt nach dem Beginn, höchstens 24 Stunden später.".into()));
            }
            Some(end)
        }
        (Some(start), None) => Some(start + Duration::minutes(i64::from(duration.unwrap_or(30)))),
    };
    sqlx::query("UPDATE tasks SET scheduled_start = $2, scheduled_end = $3, updated_at = now(), version = version + 1 WHERE id = $1")
        .bind(task_id)
        .bind(req.scheduled_start)
        .bind(end)
        .execute(&mut *tx)
        .await?;
    record_event(&mut tx, task_id, me, "scheduled", serde_json::json!({ "start": req.scheduled_start })).await?;
    tx.commit().await?;
    state.live.notify(hid, r#"{"type":"tasks_changed"}"#);
    let hh = household(&state.db, hid).await?;
    Ok(Json(load_dto(&state, task_id, &hh.policy).await?))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct AssignRequest {
    /// None = open for anyone.
    pub assignee_id: Option<Uuid>,
}

/// Board: a card dropped into someone's zone. Only while nobody is working on it.
pub async fn assign(
    State(state): State<AppState>,
    user: AuthUser,
    Path(task_id): Path<Uuid>,
    Json(req): Json<AssignRequest>,
) -> AppResult<Json<TaskDto>> {
    let (hid, _, _) = task_household(&state, &user, task_id).await?;
    let me = planner(&state, &user, hid).await?;
    let mut tx = state.db.begin().await?;
    let (status, title, current): (String, String, Option<Uuid>) =
        sqlx::query_as("SELECT status, title, assignee_id FROM tasks WHERE id = $1 FOR UPDATE").bind(task_id).fetch_one(&mut *tx).await?;
    if !["open", "claimed"].contains(&status.as_str()) {
        return Err(AppError::Conflict("Diese Aufgabe läuft gerade oder ist schon fertig.".into()));
    }
    if let Some(to) = req.assignee_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM memberships WHERE id = $1 AND household_id = $2 AND active)")
            .bind(to)
            .bind(hid)
            .fetch_one(&mut *tx)
            .await?;
        if !ok {
            return Err(AppError::BadRequest("Diese Person ist nicht im Haushalt.".into()));
        }
    }
    sqlx::query("UPDATE tasks SET assignee_id = $2, status = $3, updated_at = now(), version = version + 1 WHERE id = $1")
        .bind(task_id)
        .bind(req.assignee_id)
        .bind(if req.assignee_id.is_some() { "claimed" } else { "open" })
        .execute(&mut *tx)
        .await?;
    record_event(&mut tx, task_id, me, "assigned", serde_json::json!({ "to": req.assignee_id })).await?;
    if let Some(to) = req.assignee_id
        && to != me
        && current != Some(to)
    {
        let who = inbox::name(&mut tx, me).await?;
        inbox::add(&mut tx, Note::new(to, "assigned", Category::Tasks, format!("Neue Aufgabe für dich: „{title}“")).body(format!("von {who}")).about("task", task_id))
            .await?;
    }
    tx.commit().await?;
    state.live.notify(hid, r#"{"type":"tasks_changed"}"#);
    let hh = household(&state.db, hid).await?;
    Ok(Json(load_dto(&state, task_id, &hh.policy).await?))
}

// ---------------------------------------------------------------- calendar

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct CalendarEntry {
    /// The current round; None for a coming round that does not exist yet.
    pub task_id: Option<Uuid>,
    pub template_id: Option<Uuid>,
    pub title: String,
    pub icon: Option<String>,
    pub room_id: Option<Uuid>,
    pub kind: TaskKind,
    pub assignee_id: Option<Uuid>,
    pub xp: i64,
    pub status: String,
    pub due_at: Option<DateTime<Utc>>,
    pub scheduled_start: Option<DateTime<Utc>>,
    pub scheduled_end: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    /// A coming round, calculated from the rhythm (assuming each is done on time).
    pub projected: bool,
}

#[derive(Deserialize)]
pub struct CalendarQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(sqlx::FromRow)]
struct Row {
    id: Uuid,
    template_id: Option<Uuid>,
    title: String,
    icon: Option<String>,
    room_id: Option<Uuid>,
    kind: String,
    assignee_id: Option<Uuid>,
    points_default: i64,
    status: String,
    due_at: Option<DateTime<Utc>>,
    scheduled_start: Option<DateTime<Utc>>,
    scheduled_end: Option<DateTime<Utc>>,
    completed_at: Option<DateTime<Utc>>,
    points_awarded: Option<i64>,
    schedule: Option<serde_json::Value>,
    assignment: Option<serde_json::Value>,
}

fn kind(s: &str) -> TaskKind {
    match s {
        "reward" => TaskKind::Reward,
        "penalty" => TaskKind::Penalty,
        _ => TaskKind::Regular,
    }
}

/// Rounds between `from` and `to`: current ones (by due date or planned time), done ones
/// (by completion), and the coming ones of recurring tasks.
pub async fn calendar(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Query(q): Query<CalendarQuery>,
) -> AppResult<Json<Vec<CalendarEntry>>> {
    membership(&state, user.user_id, household_id).await?;
    if q.to <= q.from || q.to - q.from > Duration::days(62) {
        return Err(AppError::BadRequest("Zeitraum: höchstens 62 Tage.".into()));
    }
    let hh = household(&state.db, household_id).await?;
    let rows = sqlx::query_as::<_, Row>(
        "SELECT t.id, t.template_id, t.title, t.icon, t.room_id, t.kind, t.assignee_id, t.points_default, t.status, t.due_at,
                t.scheduled_start, t.scheduled_end, t.completed_at, t.points_awarded, tt.schedule, tt.assignment
         FROM tasks t LEFT JOIN task_templates tt ON tt.id = t.template_id AND NOT tt.archived
         WHERE t.household_id = $1 AND (
               (t.status NOT IN ('completed', 'cancelled'))
            OR (t.status = 'completed' AND t.completed_at >= $2 AND t.completed_at < $3))",
    )
    .bind(household_id)
    .bind(q.from)
    .bind(q.to)
    .fetch_all(&state.db)
    .await?;

    let mut out = Vec::new();
    for r in rows {
        let open = !["completed", "cancelled"].contains(&r.status.as_str());
        let at = if open { r.scheduled_start.or(r.due_at) } else { r.completed_at };
        // Open rounds without a date (one-off, unplanned) stay in the lists, not the calendar.
        if let Some(at) = at
            && (at >= q.from && at < q.to || open && at < q.from && r.due_at.is_some_and(|d| d < q.to))
        {
            out.push(CalendarEntry {
                task_id: Some(r.id),
                template_id: r.template_id,
                title: r.title.clone(),
                icon: r.icon.clone(),
                room_id: r.room_id,
                kind: kind(&r.kind),
                assignee_id: r.assignee_id,
                xp: r.points_awarded.unwrap_or(r.points_default),
                status: r.status.clone(),
                due_at: r.due_at,
                scheduled_start: r.scheduled_start,
                scheduled_end: r.scheduled_end,
                completed_at: r.completed_at,
                projected: false,
            });
        }
        // Coming rounds of the current one.
        let (Some(due), true, Some(schedule)) = (r.due_at, open, r.schedule.as_ref()) else { continue };
        let Ok(schedule) = serde_json::from_value::<Schedule>(schedule.clone()) else { continue };
        let assignment = r.assignment.clone().and_then(|a| serde_json::from_value::<Assignment>(a).ok()).unwrap_or(Assignment::Open);
        for (i, cycle) in schedule.upcoming(due, q.to, hh.tz, 40).into_iter().enumerate() {
            if cycle.due < q.from {
                continue;
            }
            let assignee = match &assignment {
                Assignment::Fixed { member } => Some(*member),
                Assignment::Rotation { members, next } if !members.is_empty() => Some(members[(next + i) % members.len()]),
                _ => None,
            };
            out.push(CalendarEntry {
                task_id: None,
                template_id: r.template_id,
                title: r.title.clone(),
                icon: r.icon.clone(),
                room_id: r.room_id,
                kind: kind(&r.kind),
                assignee_id: assignee,
                xp: r.points_default,
                status: "projected".into(),
                due_at: Some(cycle.due),
                scheduled_start: None,
                scheduled_end: None,
                completed_at: None,
                projected: true,
            });
        }
    }
    out.sort_by_key(|e| e.scheduled_start.or(e.completed_at).or(e.due_at));
    Ok(Json(out))
}

// ---------------------------------------------------------------- board

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, sqlx::FromRow)]
pub struct BoardItemDto {
    pub task_id: Option<Uuid>,
    pub template_id: Option<Uuid>,
    pub x: f64,
    pub y: f64,
}

async fn shared_board(db: &sqlx::PgPool, household_id: Uuid) -> AppResult<Uuid> {
    sqlx::query("INSERT INTO boards (id, household_id, name) VALUES ($1, $2, 'Board') ON CONFLICT (household_id) WHERE owner_id IS NULL DO NOTHING")
        .bind(Uuid::new_v4())
        .bind(household_id)
        .execute(db)
        .await?;
    Ok(sqlx::query_scalar("SELECT id FROM boards WHERE household_id = $1 AND owner_id IS NULL").bind(household_id).fetch_one(db).await?)
}

/// Card positions on the shared board (recurring tasks keep theirs from round to round).
pub async fn board(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<Vec<BoardItemDto>>> {
    membership(&state, user.user_id, household_id).await?;
    let board = shared_board(&state.db, household_id).await?;
    Ok(Json(
        sqlx::query_as::<_, BoardItemDto>("SELECT task_id, template_id, x, y FROM board_items WHERE board_id = $1").bind(board).fetch_all(&state.db).await?,
    ))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct BoardMoveRequest {
    pub task_id: Uuid,
    pub x: f64,
    pub y: f64,
}

pub async fn move_card(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<BoardMoveRequest>,
) -> AppResult<StatusCode> {
    planner(&state, &user, household_id).await?;
    if !req.x.is_finite() || !req.y.is_finite() || req.x.abs() > 20_000.0 || req.y.abs() > 20_000.0 {
        return Err(AppError::BadRequest("Position außerhalb des Boards.".into()));
    }
    let template: Option<Option<Uuid>> = sqlx::query_scalar("SELECT template_id FROM tasks WHERE id = $1 AND household_id = $2")
        .bind(req.task_id)
        .bind(household_id)
        .fetch_optional(&state.db)
        .await?;
    let template = template.ok_or(AppError::NotFound)?;
    let board = shared_board(&state.db, household_id).await?;
    let sql = if template.is_some() {
        "INSERT INTO board_items (id, board_id, type, task_id, template_id, x, y) VALUES ($1, $2, 'task', $3, $4, $5, $6)
         ON CONFLICT (board_id, template_id) WHERE template_id IS NOT NULL DO UPDATE SET task_id = $3, x = $5, y = $6"
    } else {
        "INSERT INTO board_items (id, board_id, type, task_id, template_id, x, y) VALUES ($1, $2, 'task', $3, $4, $5, $6)
         ON CONFLICT (board_id, task_id) WHERE template_id IS NULL AND task_id IS NOT NULL DO UPDATE SET x = $5, y = $6"
    };
    sqlx::query(sql)
        .bind(Uuid::new_v4())
        .bind(board)
        .bind(req.task_id)
        .bind(template)
        .bind(req.x)
        .bind(req.y)
        .execute(&state.db)
        .await?;
    state.live.notify(household_id, r#"{"type":"board_changed"}"#);
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- saved views

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct ViewFilter {
    /// Assigned to one of these (empty = everybody); `open` = include unassigned.
    #[serde(default)]
    pub members: Vec<Uuid>,
    #[serde(default)]
    pub open: bool,
    #[serde(default)]
    pub rooms: Vec<Uuid>,
    /// Task states, e.g. ["open", "claimed"] (empty = all current ones).
    #[serde(default)]
    pub statuses: Vec<String>,
    #[serde(default)]
    pub kinds: Vec<TaskKind>,
    #[serde(default)]
    pub bounty_only: bool,
    /// Due within this many days (overdue included).
    #[serde(default)]
    pub due_within_days: Option<u32>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct ViewSort {
    /// `condition` (most urgent first), `due`, `xp`, `title`.
    pub by: String,
    /// `none`, `member`, `room`, `status`.
    pub group: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct ViewDto {
    pub id: Uuid,
    pub owner_id: Uuid,
    pub name: String,
    pub shared: bool,
    pub filter: ViewFilter,
    /// `list`, `kanban`, `calendar`, `canvas`.
    pub layout: String,
    pub sort: ViewSort,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct ViewRequest {
    pub name: String,
    pub shared: bool,
    pub filter: ViewFilter,
    pub layout: String,
    pub sort: ViewSort,
}

#[derive(sqlx::FromRow)]
struct ViewRow {
    id: Uuid,
    owner_id: Uuid,
    name: String,
    shared: bool,
    filter: serde_json::Value,
    layout: String,
    sort: serde_json::Value,
}

fn view_dto(r: ViewRow) -> ViewDto {
    ViewDto {
        id: r.id,
        owner_id: r.owner_id,
        name: r.name,
        shared: r.shared,
        filter: serde_json::from_value(r.filter).unwrap_or_default(),
        layout: r.layout,
        sort: serde_json::from_value(r.sort).unwrap_or(ViewSort { by: "condition".into(), group: "none".into() }),
    }
}

fn check_view(req: &ViewRequest) -> AppResult<String> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return Err(AppError::BadRequest("Der Name braucht 1 bis 40 Zeichen.".into()));
    }
    let statuses_ok = req.filter.statuses.iter().all(|s| ["open", "claimed", "in_progress", "submitted"].contains(&s.as_str()));
    if !["list", "kanban", "calendar", "canvas"].contains(&req.layout.as_str())
        || !["condition", "due", "xp", "title"].contains(&req.sort.by.as_str())
        || !["none", "member", "room", "status"].contains(&req.sort.group.as_str())
        || !statuses_ok
        || req.filter.due_within_days.is_some_and(|d| d > 365)
    {
        return Err(AppError::BadRequest("Unbekannte Darstellung, Sortierung oder Filter.".into()));
    }
    Ok(name.to_string())
}

/// Your own views and the ones shared with the household.
pub async fn views(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<Vec<ViewDto>>> {
    let (me, _) = membership(&state, user.user_id, household_id).await?;
    let rows = sqlx::query_as::<_, ViewRow>(
        "SELECT id, owner_id, name, shared, filter, layout, sort FROM saved_views
         WHERE household_id = $1 AND (owner_id = $2 OR shared) ORDER BY created_at",
    )
    .bind(household_id)
    .bind(me)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().map(view_dto).collect()))
}

pub async fn create_view(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<ViewRequest>,
) -> AppResult<(StatusCode, Json<ViewDto>)> {
    let (me, _) = membership(&state, user.user_id, household_id).await?;
    let name = check_view(&req)?;
    let row = sqlx::query_as::<_, ViewRow>(
        "INSERT INTO saved_views (id, household_id, owner_id, name, shared, filter, layout, sort) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         RETURNING id, owner_id, name, shared, filter, layout, sort",
    )
    .bind(Uuid::new_v4())
    .bind(household_id)
    .bind(me)
    .bind(name)
    .bind(req.shared)
    .bind(serde_json::to_value(&req.filter).map_err(anyhow::Error::from)?)
    .bind(&req.layout)
    .bind(serde_json::to_value(&req.sort).map_err(anyhow::Error::from)?)
    .fetch_one(&state.db)
    .await?;
    if req.shared {
        state.live.notify(household_id, r#"{"type":"views_changed"}"#);
    }
    Ok((StatusCode::CREATED, Json(view_dto(row))))
}

async fn own_view(state: &AppState, user: &AuthUser, view_id: Uuid, allow_admin: bool) -> AppResult<Uuid> {
    let (household_id, owner, shared): (Uuid, Uuid, bool) = sqlx::query_as("SELECT household_id, owner_id, shared FROM saved_views WHERE id = $1")
        .bind(view_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let (me, role) = membership(state, user.user_id, household_id).await?;
    if owner != me && !(allow_admin && shared && role == "admin") {
        return Err(if shared { AppError::Forbidden("Diese Ansicht gehört jemand anderem.".into()) } else { AppError::NotFound });
    }
    Ok(household_id)
}

pub async fn update_view(State(state): State<AppState>, user: AuthUser, Path(view_id): Path<Uuid>, Json(req): Json<ViewRequest>) -> AppResult<Json<ViewDto>> {
    let household_id = own_view(&state, &user, view_id, false).await?;
    let name = check_view(&req)?;
    let row = sqlx::query_as::<_, ViewRow>(
        "UPDATE saved_views SET name = $2, shared = $3, filter = $4, layout = $5, sort = $6 WHERE id = $1
         RETURNING id, owner_id, name, shared, filter, layout, sort",
    )
    .bind(view_id)
    .bind(name)
    .bind(req.shared)
    .bind(serde_json::to_value(&req.filter).map_err(anyhow::Error::from)?)
    .bind(&req.layout)
    .bind(serde_json::to_value(&req.sort).map_err(anyhow::Error::from)?)
    .fetch_one(&state.db)
    .await?;
    state.live.notify(household_id, r#"{"type":"views_changed"}"#);
    Ok(Json(view_dto(row)))
}

/// Owners delete their views; admins may also remove shared ones.
pub async fn delete_view(State(state): State<AppState>, user: AuthUser, Path(view_id): Path<Uuid>) -> AppResult<StatusCode> {
    let household_id = own_view(&state, &user, view_id, true).await?;
    sqlx::query("DELETE FROM saved_views WHERE id = $1").bind(view_id).execute(&state.db).await?;
    state.live.notify(household_id, r#"{"type":"views_changed"}"#);
    Ok(StatusCode::NO_CONTENT)
}
