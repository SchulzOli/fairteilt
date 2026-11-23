//! Inbox, notification settings and push devices (M5).

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, NaiveTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::households::membership;
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::inbox::{PushCategories, push_prefs};
use crate::AppState;

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, sqlx::FromRow)]
pub struct NotificationDto {
    pub id: Uuid,
    /// e.g. `assigned`, `review_requested`, `approved`, `received`, `due`, `period_warning`.
    pub kind: String,
    pub category: String,
    pub title: String,
    pub body: Option<String>,
    /// What to open: `task`, `settlement`, `transfer`, `redemption`, `ledger`.
    pub ref_type: Option<String>,
    pub ref_id: Option<Uuid>,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct InboxQuery {
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

pub async fn list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Query(q): Query<InboxQuery>,
) -> AppResult<Json<Vec<NotificationDto>>> {
    let (me, _) = membership(&state, user.user_id, household_id).await?;
    Ok(Json(
        sqlx::query_as::<_, NotificationDto>(
            "SELECT id, type AS kind, category, title, body, ref_type, ref_id, read_at, created_at FROM notifications
             WHERE member_id = $1 AND ($2::timestamptz IS NULL OR created_at < $2) ORDER BY created_at DESC LIMIT $3",
        )
        .bind(me)
        .bind(q.before)
        .bind(q.limit.unwrap_or(30).clamp(1, 100))
        .fetch_all(&state.db)
        .await?,
    ))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct UnreadDto {
    pub unread: i64,
}

pub async fn unread(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<UnreadDto>> {
    let (me, _) = membership(&state, user.user_id, household_id).await?;
    let unread: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE member_id = $1 AND read_at IS NULL")
        .bind(me)
        .fetch_one(&state.db)
        .await?;
    Ok(Json(UnreadDto { unread }))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct ReadRequest {
    /// These entries; None = everything.
    pub ids: Option<Vec<Uuid>>,
}

pub async fn mark_read(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<ReadRequest>,
) -> AppResult<Json<UnreadDto>> {
    let (me, _) = membership(&state, user.user_id, household_id).await?;
    sqlx::query("UPDATE notifications SET read_at = now() WHERE member_id = $1 AND read_at IS NULL AND ($2::uuid[] IS NULL OR id = ANY($2))")
        .bind(me)
        .bind(req.ids)
        .execute(&state.db)
        .await?;
    unread(State(state), user, Path(household_id)).await
}

// ---------------------------------------------------------------- settings

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, Deserialize)]
pub struct NotificationPrefsDto {
    /// Quiet hours, local time "22:00"–"07:00" (pushes wait until they end). None = off.
    pub quiet_from: Option<String>,
    pub quiet_to: Option<String>,
    pub push: PushCategories,
}

fn fmt(t: Option<NaiveTime>) -> Option<String> {
    t.map(|t| t.format("%H:%M").to_string())
}

fn parse(t: &Option<String>) -> AppResult<Option<NaiveTime>> {
    match t.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(None),
        Some(s) => NaiveTime::parse_from_str(s, "%H:%M").map(Some).map_err(|_| AppError::BadRequest("Uhrzeit bitte als HH:MM.".into())),
    }
}

pub async fn prefs(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<NotificationPrefsDto>> {
    let (me, _) = membership(&state, user.user_id, household_id).await?;
    let mut conn = state.db.acquire().await?;
    let (push, from, to) = push_prefs(&mut conn, me).await?;
    Ok(Json(NotificationPrefsDto { quiet_from: fmt(from), quiet_to: fmt(to), push }))
}

pub async fn update_prefs(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<NotificationPrefsDto>,
) -> AppResult<Json<NotificationPrefsDto>> {
    let (me, _) = membership(&state, user.user_id, household_id).await?;
    let (from, to) = (parse(&req.quiet_from)?, parse(&req.quiet_to)?);
    if from.is_some() != to.is_some() {
        return Err(AppError::BadRequest("Für Ruhezeiten bitte Beginn und Ende angeben.".into()));
    }
    sqlx::query(
        "INSERT INTO notification_prefs (member_id, quiet_from, quiet_to, channels) VALUES ($1, $2, $3, $4)
         ON CONFLICT (member_id) DO UPDATE SET quiet_from = $2, quiet_to = $3, channels = $4",
    )
    .bind(me)
    .bind(from)
    .bind(to)
    .bind(serde_json::json!({ "push": req.push }))
    .execute(&state.db)
    .await?;
    Ok(Json(NotificationPrefsDto { quiet_from: fmt(from), quiet_to: fmt(to), push: req.push }))
}

// ---------------------------------------------------------------- devices

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct DeviceRequest {
    /// `android`, `ios`, `windows`, `macos`, `linux`.
    pub platform: String,
    /// `fcm` (Android), later `apns` / `unifiedpush`.
    pub push_provider: String,
    pub push_token: String,
}

/// The app registers its push token after sign-in (and when the token changes). A token
/// belongs to one user: signing in with another account on the same phone moves it.
pub async fn register_device(State(state): State<AppState>, user: AuthUser, Json(req): Json<DeviceRequest>) -> AppResult<StatusCode> {
    if !["android", "ios", "windows", "macos", "linux"].contains(&req.platform.as_str())
        || !["fcm", "apns", "unifiedpush"].contains(&req.push_provider.as_str())
        || req.push_token.is_empty()
        || req.push_token.len() > 4096
    {
        return Err(AppError::BadRequest("Unbekanntes Gerät oder ungültiges Push-Token.".into()));
    }
    sqlx::query(
        "INSERT INTO devices (id, user_id, platform, push_provider, push_token) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (push_token) WHERE push_token IS NOT NULL
         DO UPDATE SET user_id = $2, platform = $3, push_provider = $4, last_seen_at = now()",
    )
    .bind(Uuid::new_v4())
    .bind(user.user_id)
    .bind(&req.platform)
    .bind(&req.push_provider)
    .bind(&req.push_token)
    .execute(&state.db)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct UnregisterDevice {
    pub push_token: String,
}

/// Before signing out: no more pushes to this phone for this account.
pub async fn unregister_device(State(state): State<AppState>, user: AuthUser, Json(req): Json<UnregisterDevice>) -> AppResult<StatusCode> {
    sqlx::query("DELETE FROM devices WHERE user_id = $1 AND push_token = $2").bind(user.user_id).bind(&req.push_token).execute(&state.db).await?;
    Ok(StatusCode::NO_CONTENT)
}
