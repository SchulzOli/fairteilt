//! Households and invites.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Duration, Utc};
use punkte_core::{PeriodPolicy, PointPolicy};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{AuthUser, hash_token, new_invite_code, normalize_invite_code};
use crate::error::{AppError, AppResult};
use crate::AppState;

const ANIMALS: &[&str] = &["fox", "owl", "otter", "hedgehog", "bear", "cat", "dog", "whale", "lion"];
const ROLES: &[&str] = &["admin", "member", "child"];
const INVITE_TTL_DAYS: i64 = 7;

fn check_animal(animal: &Option<String>) -> AppResult<String> {
    let a = animal.clone().unwrap_or_else(|| "fox".into());
    if ANIMALS.contains(&a.as_str()) { Ok(a) } else { Err(AppError::BadRequest("Dieses Tier gibt es nicht.".into())) }
}

fn check_color(color: &Option<String>) -> AppResult<String> {
    let c = color.clone().unwrap_or_else(|| "#2563eb".into());
    let ok = c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|ch| ch.is_ascii_hexdigit());
    if ok { Ok(c.to_lowercase()) } else { Err(AppError::BadRequest("Farbe bitte als #rrggbb angeben.".into())) }
}

#[derive(Deserialize)]
pub struct CreateHouseholdRequest {
    pub name: String,
    pub color: Option<String>,
    pub animal: Option<String>,
}

#[derive(Serialize)]
pub struct CreateHouseholdResponse {
    pub household_id: Uuid,
    pub membership_id: Uuid,
}

/// Creates a household with the default rules (KONZEPT §13); the creator becomes admin.
pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<CreateHouseholdRequest>,
) -> AppResult<(StatusCode, Json<CreateHouseholdResponse>)> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 60 {
        return Err(AppError::BadRequest("Der Name des Haushalts muss 1 bis 60 Zeichen lang sein.".into()));
    }
    let color = check_color(&req.color)?;
    let animal = check_animal(&req.animal)?;
    let household_id = Uuid::new_v4();
    let membership_id = Uuid::new_v4();
    let point_policy = serde_json::to_value(PointPolicy::default()).map_err(anyhow::Error::from)?;
    let period_policy = serde_json::to_value(PeriodPolicy::default()).map_err(anyhow::Error::from)?;

    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO households (id, name, point_policy, period_policy) VALUES ($1, $2, $3, $4)")
        .bind(household_id)
        .bind(name)
        .bind(point_policy)
        .bind(period_policy)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO memberships (id, household_id, user_id, role, color, animal) VALUES ($1, $2, $3, 'admin', $4, $5)")
        .bind(membership_id)
        .bind(household_id)
        .bind(user.user_id)
        .bind(color)
        .bind(animal)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(CreateHouseholdResponse { household_id, membership_id })))
}

/// The caller's active membership in a household: (membership id, role).
async fn membership(state: &AppState, user_id: Uuid, household_id: Uuid) -> AppResult<(Uuid, String)> {
    sqlx::query_as("SELECT id, role FROM memberships WHERE household_id = $1 AND user_id = $2 AND active")
        .bind(household_id)
        .bind(user_id)
        .fetch_optional(&state.db)
        .await?
        // Not a member: answer as if the household did not exist.
        .ok_or(AppError::NotFound)
}

#[derive(Deserialize)]
pub struct CreateInviteRequest {
    pub role: String,
}

#[derive(Serialize)]
pub struct InviteResponse {
    /// Shown once; only its hash is stored.
    pub code: String,
    pub role: String,
    pub expires_at: DateTime<Utc>,
}

pub async fn create_invite(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<CreateInviteRequest>,
) -> AppResult<(StatusCode, Json<InviteResponse>)> {
    let (membership_id, role) = membership(&state, user.user_id, household_id).await?;
    if role != "admin" {
        return Err(AppError::Forbidden("Nur Admins können einladen.".into()));
    }
    if !ROLES.contains(&req.role.as_str()) {
        return Err(AppError::BadRequest("Rolle muss admin, member oder child sein.".into()));
    }
    let code = new_invite_code();
    let expires_at = Utc::now() + Duration::days(INVITE_TTL_DAYS);
    sqlx::query("INSERT INTO invites (id, household_id, code_hash, role, created_by, expires_at) VALUES ($1, $2, $3, $4, $5, $6)")
        .bind(Uuid::new_v4())
        .bind(household_id)
        .bind(hash_token(&code))
        .bind(&req.role)
        .bind(membership_id)
        .bind(expires_at)
        .execute(&state.db)
        .await?;
    Ok((StatusCode::CREATED, Json(InviteResponse { code, role: req.role, expires_at })))
}

#[derive(Deserialize)]
pub struct AcceptInviteRequest {
    pub code: String,
    pub color: Option<String>,
    pub animal: Option<String>,
}

#[derive(Serialize)]
pub struct AcceptInviteResponse {
    pub household_id: Uuid,
    pub membership_id: Uuid,
    pub role: String,
}

pub async fn accept_invite(
    State(state): State<AppState>,
    user: AuthUser,
    Json(req): Json<AcceptInviteRequest>,
) -> AppResult<(StatusCode, Json<AcceptInviteResponse>)> {
    let invalid = || AppError::BadRequest("Dieser Einladungscode ist ungültig oder abgelaufen.".into());
    let color = check_color(&req.color)?;
    let animal = check_animal(&req.animal)?;
    let code_hash = hash_token(&normalize_invite_code(&req.code));

    let mut tx = state.db.begin().await?;
    // Lock the invite so it cannot be used twice concurrently.
    let invite: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT id, household_id, role FROM invites
         WHERE code_hash = $1 AND used_by IS NULL AND expires_at > now()
         FOR UPDATE",
    )
    .bind(code_hash)
    .fetch_optional(&mut *tx)
    .await?;
    let (invite_id, household_id, role) = invite.ok_or_else(invalid)?;

    let already: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM memberships WHERE household_id = $1 AND user_id = $2)")
        .bind(household_id)
        .bind(user.user_id)
        .fetch_one(&mut *tx)
        .await?;
    if already {
        return Err(AppError::Conflict("Du bist schon Mitglied in diesem Haushalt.".into()));
    }

    let membership_id = Uuid::new_v4();
    let weight = if role == "child" { 0.3 } else { 1.0 };
    sqlx::query(
        "INSERT INTO memberships (id, household_id, user_id, role, weight, color, animal) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(membership_id)
    .bind(household_id)
    .bind(user.user_id)
    .bind(&role)
    .bind(weight)
    .bind(color)
    .bind(animal)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE invites SET used_by = $2, used_at = now() WHERE id = $1")
        .bind(invite_id)
        .bind(user.user_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(AcceptInviteResponse { household_id, membership_id, role })))
}
