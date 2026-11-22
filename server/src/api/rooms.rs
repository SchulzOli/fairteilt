//! Rooms: how tasks are grouped (KONZEPT §7.1, like Tody).

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::households::membership;
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::AppState;

/// Rooms every new household starts with.
pub const DEFAULT_ROOMS: &[(&str, &str)] =
    &[("Küche", "kitchen"), ("Bad", "bath"), ("Wohnzimmer", "sofa"), ("Wäsche", "shirt"), ("Flur", "bin")];

/// Icon names of the app's icon family (lowercase letters and dashes, max 20).
pub fn check_icon(icon: &Option<String>) -> AppResult<Option<String>> {
    match icon {
        None => Ok(None),
        Some(i) if !i.is_empty() && i.len() <= 20 && i.chars().all(|c| c.is_ascii_lowercase() || c == '-') => Ok(Some(i.clone())),
        Some(_) => Err(AppError::BadRequest("Unbekanntes Symbol.".into())),
    }
}

fn check_room_name(name: &str) -> AppResult<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 40 {
        return Err(AppError::BadRequest("Der Raumname muss 1 bis 40 Zeichen lang sein.".into()));
    }
    Ok(n.to_string())
}

/// Members and admins may manage rooms and tasks; children use them.
pub(crate) async fn planner(state: &AppState, user: &AuthUser, household_id: Uuid) -> AppResult<Uuid> {
    let (mid, role) = membership(state, user.user_id, household_id).await?;
    if role == "child" {
        return Err(AppError::Forbidden("Kinder können das nicht ändern. Frag bitte deine Eltern.".into()));
    }
    Ok(mid)
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, sqlx::FromRow)]
pub struct RoomDto {
    pub id: Uuid,
    pub name: String,
    pub icon: Option<String>,
    pub position: i32,
}

pub async fn list(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<Vec<RoomDto>>> {
    membership(&state, user.user_id, household_id).await?;
    let rows = sqlx::query_as::<_, RoomDto>("SELECT id, name, icon, position FROM rooms WHERE household_id = $1 ORDER BY position, name")
        .bind(household_id)
        .fetch_all(&state.db)
        .await?;
    Ok(Json(rows))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct RoomRequest {
    pub name: String,
    pub icon: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<RoomRequest>,
) -> AppResult<(StatusCode, Json<RoomDto>)> {
    planner(&state, &user, household_id).await?;
    let name = check_room_name(&req.name)?;
    let icon = check_icon(&req.icon)?;
    let room = sqlx::query_as::<_, RoomDto>(
        "INSERT INTO rooms (id, household_id, name, icon, position)
         VALUES ($1, $2, $3, $4, (SELECT COALESCE(MAX(position) + 1, 0) FROM rooms WHERE household_id = $2))
         RETURNING id, name, icon, position",
    )
    .bind(Uuid::new_v4())
    .bind(household_id)
    .bind(name)
    .bind(icon)
    .fetch_one(&state.db)
    .await?;
    state.live.notify(household_id, r#"{"type":"rooms_changed"}"#);
    Ok((StatusCode::CREATED, Json(room)))
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path((household_id, room_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<RoomRequest>,
) -> AppResult<StatusCode> {
    planner(&state, &user, household_id).await?;
    let name = check_room_name(&req.name)?;
    let icon = check_icon(&req.icon)?;
    let r = sqlx::query("UPDATE rooms SET name = $3, icon = $4 WHERE id = $1 AND household_id = $2")
        .bind(room_id)
        .bind(household_id)
        .bind(name)
        .bind(icon)
        .execute(&state.db)
        .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    state.live.notify(household_id, r#"{"type":"rooms_changed"}"#);
    Ok(StatusCode::NO_CONTENT)
}

/// Deleting a room keeps its tasks (they become room-less).
pub async fn delete(State(state): State<AppState>, user: AuthUser, Path((household_id, room_id)): Path<(Uuid, Uuid)>) -> AppResult<StatusCode> {
    planner(&state, &user, household_id).await?;
    let r = sqlx::query("DELETE FROM rooms WHERE id = $1 AND household_id = $2").bind(room_id).bind(household_id).execute(&state.db).await?;
    if r.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    state.live.notify(household_id, r#"{"type":"rooms_changed"}"#);
    Ok(StatusCode::NO_CONTENT)
}
