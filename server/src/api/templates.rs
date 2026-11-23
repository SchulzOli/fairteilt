//! Editing what was set up: change a task's setup (template), delete it, move it to another
//! room. The current round follows right away as far as it makes sense:
//! - name, symbol, room: always (also while someone works on it)
//! - points, steps, duration, rhythm, who is on it: only while nobody has started it yet;
//!   steps only while none of them is checked off. Otherwise from the next round on.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use punkte_core::{Assignment, Cycle, Schedule};
use serde::Deserialize;
use uuid::Uuid;

use crate::api::rooms::planner;
use crate::api::tasks::{
    CreateTemplateRequest, CheckedTemplate, active_members, check_room_and_people, check_template, household, kind_str, lock_task,
    month_load, record_event, refund_bounties, spawn_round, task_household,
};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::AppState;

async fn template_household(state: &AppState, template_id: Uuid) -> AppResult<Uuid> {
    sqlx::query_scalar("SELECT household_id FROM task_templates WHERE id = $1 AND NOT archived")
        .bind(template_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)
}

/// The template's list, if it is the task of a shopping list.
async fn list_of(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, template_id: Uuid) -> AppResult<Option<String>> {
    Ok(sqlx::query_scalar("SELECT name FROM shopping_lists WHERE template_id = $1 AND NOT archived LIMIT 1")
        .bind(template_id)
        .fetch_optional(&mut **tx)
        .await?)
}

/// PATCH /templates/{tid}: the whole setup again, as when creating it.
pub async fn update_template(
    State(state): State<AppState>,
    user: AuthUser,
    Path(template_id): Path<Uuid>,
    Json(req): Json<CreateTemplateRequest>,
) -> AppResult<StatusCode> {
    let hid = template_household(&state, template_id).await?;
    let me = planner(&state, &user, hid).await?;
    let hh = household(&state.db, hid).await?;
    let CheckedTemplate { title, range, steps, icon } = check_template(&req, &hh)?;
    let mut tx = state.db.begin().await?;
    check_room_and_people(&mut tx, hid, &req).await?;

    let (old_schedule, old_assignment): (serde_json::Value, serde_json::Value) =
        sqlx::query_as("SELECT schedule, assignment FROM task_templates WHERE id = $1 FOR UPDATE").bind(template_id).fetch_one(&mut *tx).await?;
    let old_schedule: Schedule = serde_json::from_value(old_schedule).unwrap_or(Schedule::Once);
    if list_of(&mut tx, template_id).await?.is_some() && !req.schedule.is_on_call() {
        return Err(AppError::BadRequest("Einkaufen bleibt „auf Abruf“: Die Liste ruft die Aufgabe, sobald etwas draufsteht.".into()));
    }
    let schedule_json = serde_json::to_value(&req.schedule).map_err(anyhow::Error::from)?;
    let assignment_json = serde_json::to_value(&req.assignment).map_err(anyhow::Error::from)?;
    let schedule_changed = serde_json::to_value(&old_schedule).map_err(anyhow::Error::from)? != schedule_json;
    let assignment_changed = old_assignment != assignment_json;

    sqlx::query(
        "UPDATE task_templates SET room_id = $2, title = $3, icon = $4, kind = $5, points_min = $6, points_default = $7, points_max = $8,
                schedule = $9, assignment = $10, step_order = $11, duration_min = $12, approval = $13
         WHERE id = $1",
    )
    .bind(template_id)
    .bind(req.room_id)
    .bind(&title)
    .bind(&icon)
    .bind(kind_str(req.kind.unwrap_or(punkte_core::TaskKind::Regular)))
    .bind(range.min)
    .bind(range.default)
    .bind(range.max)
    .bind(&schedule_json)
    .bind(&assignment_json)
    .bind(req.step_order.as_str())
    .bind(req.duration_min)
    .bind(req.approval.unwrap_or_default().as_str())
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM template_steps WHERE template_id = $1").bind(template_id).execute(&mut *tx).await?;
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
    let round: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM tasks WHERE template_id = $1 AND status NOT IN ('completed', 'cancelled') ORDER BY created_at DESC LIMIT 1 FOR UPDATE",
    )
    .bind(template_id)
    .fetch_optional(&mut *tx)
    .await?;
    match round {
        Some((task_id, status)) => {
            sqlx::query("UPDATE tasks SET title = $2, icon = $3, room_id = $4, kind = $5, updated_at = now(), version = version + 1 WHERE id = $1")
                .bind(task_id)
                .bind(&title)
                .bind(&icon)
                .bind(req.room_id)
                .bind(kind_str(req.kind.unwrap_or(punkte_core::TaskKind::Regular)))
                .execute(&mut *tx)
                .await?;
            if status == "open" || status == "claimed" {
                sqlx::query("UPDATE tasks SET points_min = $2, points_default = $3, points_max = $4, step_order = $5, duration_min = $6 WHERE id = $1")
                    .bind(task_id)
                    .bind(range.min)
                    .bind(range.default)
                    .bind(range.max)
                    .bind(req.step_order.as_str())
                    .bind(req.duration_min)
                    .execute(&mut *tx)
                    .await?;
                let started: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM task_steps WHERE task_id = $1 AND done_at IS NOT NULL)")
                    .bind(task_id)
                    .fetch_one(&mut *tx)
                    .await?;
                if !started {
                    sqlx::query("DELETE FROM task_steps WHERE task_id = $1").bind(task_id).execute(&mut *tx).await?;
                    for (i, s) in steps.iter().enumerate() {
                        sqlx::query("INSERT INTO task_steps (id, task_id, position, title) VALUES ($1, $2, $3, $4)")
                            .bind(Uuid::new_v4())
                            .bind(task_id)
                            .bind(i as i32)
                            .bind(s)
                            .execute(&mut *tx)
                            .await?;
                    }
                }
                // A new rhythm: the open round gets the dates of a first round from now.
                // Rounds "auf Abruf" keep theirs (they were called already).
                if schedule_changed || matches!(req.schedule, Schedule::Once) {
                    let dates: Option<(DateTime<Utc>, Option<DateTime<Utc>>)> = match req.schedule.first_cycle(now, hh.tz) {
                        Some(Cycle { start, due }) => Some((start, Some(due))),
                        None if matches!(req.schedule, Schedule::Once) => Some((now, req.due_at)),
                        None => None,
                    };
                    if let Some((start, due)) = dates {
                        sqlx::query("UPDATE tasks SET cycle_start = $2, due_at = $3 WHERE id = $1").bind(task_id).bind(start).bind(due).execute(&mut *tx).await?;
                    }
                }
                if assignment_changed {
                    let members = active_members(&mut tx, hid).await?;
                    let load = if matches!(req.assignment, Assignment::LeastLoaded { .. }) { month_load(&mut tx, hid, hh.tz, now).await? } else { vec![] };
                    let (assignee, next) = req.assignment.pick(&load, &|m| members.contains(&m));
                    sqlx::query("UPDATE tasks SET assignee_id = $2, status = $3 WHERE id = $1")
                        .bind(task_id)
                        .bind(assignee)
                        .bind(if assignee.is_some() { "claimed" } else { "open" })
                        .execute(&mut *tx)
                        .await?;
                    if next != req.assignment {
                        sqlx::query("UPDATE task_templates SET assignment = $2 WHERE id = $1")
                            .bind(template_id)
                            .bind(serde_json::to_value(&next).map_err(anyhow::Error::from)?)
                            .execute(&mut *tx)
                            .await?;
                    }
                }
            }
            record_event(&mut tx, task_id, me, "edited", serde_json::json!({})).await?;
        }
        // No open round: a task that rested "auf Abruf" and now has a rhythm starts one.
        None if old_schedule.is_on_call() && !req.schedule.is_on_call() => {
            if let Some(Cycle { start, due }) = req.schedule.first_cycle(now, hh.tz) {
                let id = spawn_round(&mut tx, template_id, start, Some(due), now, hh.tz, Some(me)).await?;
                record_event(&mut tx, id, me, "created", serde_json::json!({})).await?;
            }
        }
        None => {}
    }
    tx.commit().await?;
    state.live.notify(hid, r#"{"type":"tasks_changed"}"#);
    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /templates/{tid}: the task is set up no more; an open round is dropped (pledged
/// Guthaben goes back). Done rounds and their points stay in the books.
pub async fn delete_template(State(state): State<AppState>, user: AuthUser, Path(template_id): Path<Uuid>) -> AppResult<StatusCode> {
    let hid = template_household(&state, template_id).await?;
    let me = planner(&state, &user, hid).await?;
    let mut tx = state.db.begin().await?;
    if let Some(list) = list_of(&mut tx, template_id).await? {
        return Err(AppError::Conflict(format!("„{list}“ einkaufen gehört zur Liste. Lösch stattdessen die Liste.")));
    }
    sqlx::query("UPDATE task_templates SET archived = true WHERE id = $1").bind(template_id).execute(&mut *tx).await?;
    let open: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM tasks WHERE template_id = $1 AND status NOT IN ('completed', 'cancelled')")
        .bind(template_id)
        .fetch_all(&mut *tx)
        .await?;
    for id in open {
        let task = lock_task(&mut tx, id).await?;
        refund_bounties(&mut tx, &task, me).await?;
        sqlx::query("UPDATE tasks SET status = 'cancelled', updated_at = now(), version = version + 1 WHERE id = $1").bind(id).execute(&mut *tx).await?;
        record_event(&mut tx, id, me, "deleted", serde_json::json!({})).await?;
    }
    tx.commit().await?;
    state.live.notify(hid, r#"{"type":"tasks_changed"}"#);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct MoveRoomRequest {
    /// None = without a room.
    pub room_id: Option<Uuid>,
}

/// PUT /tasks/{tid}/room: dragged onto another room in the plan. Moves the task and its setup.
pub async fn move_room(State(state): State<AppState>, user: AuthUser, Path(task_id): Path<Uuid>, Json(req): Json<MoveRoomRequest>) -> AppResult<StatusCode> {
    let (hid, _, _) = task_household(&state, &user, task_id).await?;
    let me = planner(&state, &user, hid).await?;
    let mut tx = state.db.begin().await?;
    if let Some(room) = req.room_id {
        let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM rooms WHERE id = $1 AND household_id = $2)")
            .bind(room)
            .bind(hid)
            .fetch_one(&mut *tx)
            .await?;
        if !ok {
            return Err(AppError::BadRequest("Diesen Raum gibt es im Haushalt nicht.".into()));
        }
    }
    let template: Option<Uuid> =
        sqlx::query_scalar("UPDATE tasks SET room_id = $2, updated_at = now(), version = version + 1 WHERE id = $1 RETURNING template_id")
            .bind(task_id)
            .bind(req.room_id)
            .fetch_one(&mut *tx)
            .await?;
    if let Some(t) = template {
        sqlx::query("UPDATE task_templates SET room_id = $2 WHERE id = $1").bind(t).bind(req.room_id).execute(&mut *tx).await?;
    }
    record_event(&mut tx, task_id, me, "moved", serde_json::json!({ "room_id": req.room_id })).await?;
    tx.commit().await?;
    state.live.notify(hid, r#"{"type":"tasks_changed"}"#);
    Ok(StatusCode::NO_CONTENT)
}
