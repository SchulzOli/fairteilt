//! Members, child profiles without e-mail, PIN login on the family tablet.
//!
//! Security model for PINs (KONZEPT §3): a PIN only works from a device that is
//! already signed in to the same household (the family tablet). Five wrong PINs lock
//! the profile for five minutes. So a 4-digit PIN cannot be guessed from the internet.

use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::auth::{TokenResponse, UserDto, device, start_session};
use crate::api::households::{check_animal, check_color, membership};
use crate::auth::{AuthUser, hash_password, hash_token, normalize_invite_code, verify_password};
use crate::error::{AppError, AppResult};
use crate::ratelimit::ClientKey;
use crate::AppState;

const PIN_MAX_FAILS: i32 = 5;
const PIN_LOCK_MINUTES: i64 = 5;

fn check_name(name: &str) -> AppResult<String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > 40 {
        return Err(AppError::BadRequest("Der Name muss 1 bis 40 Zeichen lang sein.".into()));
    }
    Ok(n.to_string())
}

fn check_pin(pin: &str) -> AppResult<()> {
    let ok = (4..=6).contains(&pin.len()) && pin.chars().all(|c| c.is_ascii_digit());
    if ok { Ok(()) } else { Err(AppError::BadRequest("Die PIN besteht aus 4 bis 6 Ziffern.".into())) }
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, sqlx::FromRow)]
pub struct MemberDto {
    pub membership_id: Uuid,
    pub user_id: Uuid,
    pub display_name: String,
    pub role: String,
    pub color: String,
    pub animal: String,
    pub weight: f64,
    /// Can sign in with a PIN on the family tablet.
    pub has_pin: bool,
    /// Has an account with e-mail (adults); false for child profiles.
    pub has_email: bool,
    pub joined_at: DateTime<Utc>,
}

/// All active members of a household (members only).
pub async fn list(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<Vec<MemberDto>>> {
    membership(&state, user.user_id, household_id).await?;
    let rows = sqlx::query_as::<_, MemberDto>(
        "SELECT m.id AS membership_id, u.id AS user_id, u.display_name, m.role, m.color, m.animal, m.weight,
                (m.pin_hash IS NOT NULL) AS has_pin, (u.email IS NOT NULL) AS has_email, m.joined_at
         FROM memberships m JOIN users u ON u.id = m.user_id
         WHERE m.household_id = $1 AND m.active
         ORDER BY m.seq",
    )
    .bind(household_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct UpdateMemberRequest {
    pub color: Option<String>,
    pub animal: Option<String>,
    /// Admins only.
    pub role: Option<String>,
    /// Admins only: capacity for the fair share (0 … 2).
    pub weight: Option<f64>,
}

/// Change avatar and colour (self or admin), role and weight (admin).
pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path((household_id, member_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<UpdateMemberRequest>,
) -> AppResult<StatusCode> {
    let (my_id, my_role) = membership(&state, user.user_id, household_id).await?;
    let is_admin = my_role == "admin";
    if member_id != my_id && !is_admin {
        return Err(AppError::Forbidden("Nur Admins können andere Mitglieder ändern.".into()));
    }
    if (req.role.is_some() || req.weight.is_some()) && !is_admin {
        return Err(AppError::Forbidden("Rolle und Gewicht können nur Admins ändern.".into()));
    }
    let color = req.color.as_ref().map(|_| check_color(&req.color)).transpose()?;
    let animal = req.animal.as_ref().map(|_| check_animal(&req.animal)).transpose()?;
    if let Some(r) = &req.role
        && !["admin", "member", "child"].contains(&r.as_str())
    {
        return Err(AppError::BadRequest("Rolle muss admin, member oder child sein.".into()));
    }
    if let Some(w) = req.weight
        && !(0.0..=2.0).contains(&w)
    {
        return Err(AppError::BadRequest("Das Gewicht liegt zwischen 0 und 2.".into()));
    }
    if req.role.as_deref().is_some_and(|r| r != "admin") && member_id == my_id {
        // Keep at least one admin: an admin may not demote themself if they are the last one.
        let admins: i64 = sqlx::query_scalar("SELECT count(*) FROM memberships WHERE household_id = $1 AND role = 'admin' AND active")
            .bind(household_id)
            .fetch_one(&state.db)
            .await?;
        if admins <= 1 {
            return Err(AppError::Conflict("Ein Haushalt braucht mindestens einen Admin.".into()));
        }
    }
    let updated = sqlx::query(
        "UPDATE memberships SET
            color = COALESCE($3, color),
            animal = COALESCE($4, animal),
            role = COALESCE($5, role),
            weight = COALESCE($6, weight)
         WHERE id = $1 AND household_id = $2 AND active",
    )
    .bind(member_id)
    .bind(household_id)
    .bind(color)
    .bind(animal)
    .bind(&req.role)
    .bind(req.weight)
    .execute(&state.db)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct CreateChildRequest {
    pub display_name: String,
    pub animal: Option<String>,
    pub color: Option<String>,
    pub pin: Option<String>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct CreatedMember {
    pub membership_id: Uuid,
    pub user_id: Uuid,
}

/// Admin creates a child profile without e-mail (for the family tablet).
pub async fn create_child(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<CreateChildRequest>,
) -> AppResult<(StatusCode, Json<CreatedMember>)> {
    let (_, role) = membership(&state, user.user_id, household_id).await?;
    if role != "admin" {
        return Err(AppError::Forbidden("Nur Admins können Kinderprofile anlegen.".into()));
    }
    let name = check_name(&req.display_name)?;
    let animal = check_animal(&req.animal)?;
    let color = check_color(&req.color)?;
    let pin_hash = match &req.pin {
        Some(pin) => {
            check_pin(pin)?;
            Some(hash_password(pin)?)
        }
        None => None,
    };
    let (user_id, membership_id) = (Uuid::new_v4(), Uuid::new_v4());
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO users (id, display_name) VALUES ($1, $2)")
        .bind(user_id)
        .bind(&name)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO memberships (id, household_id, user_id, role, weight, color, animal, pin_hash)
         VALUES ($1, $2, $3, 'child', 0.3, $4, $5, $6)",
    )
    .bind(membership_id)
    .bind(household_id)
    .bind(user_id)
    .bind(color)
    .bind(animal)
    .bind(pin_hash)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((StatusCode::CREATED, Json(CreatedMember { membership_id, user_id })))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct SetPinRequest {
    /// `null` removes the PIN.
    pub pin: Option<String>,
}

/// Set or remove a PIN: admins for anyone, everyone for themself.
pub async fn set_pin(
    State(state): State<AppState>,
    user: AuthUser,
    Path((household_id, member_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<SetPinRequest>,
) -> AppResult<StatusCode> {
    let (my_id, role) = membership(&state, user.user_id, household_id).await?;
    if member_id != my_id && role != "admin" {
        return Err(AppError::Forbidden("Nur Admins können die PIN anderer ändern.".into()));
    }
    let hash = match &req.pin {
        Some(pin) => {
            check_pin(pin)?;
            Some(hash_password(pin)?)
        }
        None => None,
    };
    let updated = sqlx::query(
        "UPDATE memberships SET pin_hash = $3, pin_failed_count = 0, pin_locked_until = NULL
         WHERE id = $1 AND household_id = $2 AND active",
    )
    .bind(member_id)
    .bind(household_id)
    .bind(hash)
    .execute(&state.db)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(sqlx::FromRow)]
struct PinTarget {
    user_id: Uuid,
    display_name: String,
    email: Option<String>,
    pin_hash: Option<String>,
    pin_locked_until: Option<DateTime<Utc>>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct PinLoginRequest {
    pub membership_id: Uuid,
    pub pin: String,
    pub device_name: Option<String>,
}

/// Profile switch on the family tablet: the tablet's own session vouches for the device,
/// the PIN for the person. Returns a session for the chosen profile.
pub async fn pin_login(State(state): State<AppState>, device_user: AuthUser, Json(req): Json<PinLoginRequest>) -> AppResult<Json<TokenResponse>> {
    let wrong = || AppError::BadRequest("Die PIN stimmt nicht.".into());
    let target = sqlx::query_as::<_, PinTarget>(
        "SELECT target.user_id, u.display_name, u.email, target.pin_hash, target.pin_locked_until
         FROM memberships target
         JOIN users u ON u.id = target.user_id
         WHERE target.id = $1 AND target.active
           AND EXISTS (SELECT 1 FROM memberships device
                       WHERE device.household_id = target.household_id AND device.user_id = $2 AND device.active)",
    )
    .bind(req.membership_id)
    .bind(device_user.user_id)
    .fetch_optional(&state.db)
    .await?
    // Unknown profile, other household or no PIN: same answer, nothing to learn.
    .ok_or_else(wrong)?;
    let pin_hash = target.pin_hash.clone().ok_or_else(wrong)?;
    if target.pin_locked_until.is_some_and(|t| t > Utc::now()) {
        return Err(AppError::TooManyRequests);
    }
    if !verify_password(req.pin.trim(), &pin_hash) {
        sqlx::query(
            "UPDATE memberships SET
                pin_failed_count = pin_failed_count + 1,
                pin_locked_until = CASE WHEN pin_failed_count + 1 >= $2 THEN now() + make_interval(mins => $3) ELSE pin_locked_until END
             WHERE id = $1",
        )
        .bind(req.membership_id)
        .bind(PIN_MAX_FAILS)
        .bind(PIN_LOCK_MINUTES as i32)
        .execute(&state.db)
        .await?;
        return Err(wrong());
    }
    sqlx::query("UPDATE memberships SET pin_failed_count = 0, pin_locked_until = NULL WHERE id = $1")
        .bind(req.membership_id)
        .execute(&state.db)
        .await?;
    let user = UserDto { id: target.user_id, email: target.email, display_name: target.display_name };
    Ok(Json(start_session(&state, user, device(req.device_name.or(Some("Familien-Tablet".into())))).await?))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct JoinRequest {
    pub code: String,
    pub display_name: String,
    pub animal: Option<String>,
    pub color: Option<String>,
    pub device_name: Option<String>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct JoinResponse {
    #[serde(flatten)]
    pub tokens: TokenResponse,
    pub household_id: Uuid,
    pub membership_id: Uuid,
}

/// A child joins on their own device with an invite code, without e-mail or password.
/// Only works with child invites; adults register with e-mail and use `/invites/accept`.
pub async fn join_without_account(
    State(state): State<AppState>,
    client: ClientKey,
    Json(req): Json<JoinRequest>,
) -> AppResult<(StatusCode, Json<JoinResponse>)> {
    state.limiter.check(&format!("invite:{}", client.0), 20, Duration::from_secs(3600))?;
    let invalid = || AppError::BadRequest("Dieser Einladungscode ist ungültig oder abgelaufen.".into());
    let name = check_name(&req.display_name)?;
    let animal = check_animal(&req.animal)?;
    let color = check_color(&req.color)?;

    let mut tx = state.db.begin().await?;
    let invite: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT id, household_id, role FROM invites WHERE code_hash = $1 AND used_by IS NULL AND expires_at > now() FOR UPDATE",
    )
    .bind(hash_token(&normalize_invite_code(&req.code)))
    .fetch_optional(&mut *tx)
    .await?;
    let (invite_id, household_id, role) = invite.ok_or_else(invalid)?;
    if role != "child" {
        return Err(AppError::BadRequest(
            "Mit diesem Code tritt man mit eigenem Konto bei. Bitte registriere dich mit E-Mail.".into(),
        ));
    }
    let (user_id, membership_id) = (Uuid::new_v4(), Uuid::new_v4());
    sqlx::query("INSERT INTO users (id, display_name) VALUES ($1, $2)").bind(user_id).bind(&name).execute(&mut *tx).await?;
    sqlx::query(
        "INSERT INTO memberships (id, household_id, user_id, role, weight, color, animal) VALUES ($1, $2, $3, 'child', 0.3, $4, $5)",
    )
    .bind(membership_id)
    .bind(household_id)
    .bind(user_id)
    .bind(color)
    .bind(animal)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE invites SET used_by = $2, used_at = now() WHERE id = $1").bind(invite_id).bind(user_id).execute(&mut *tx).await?;
    tx.commit().await?;

    let tokens = start_session(&state, UserDto { id: user_id, email: None, display_name: name }, device(req.device_name)).await?;
    Ok((StatusCode::CREATED, Json(JoinResponse { tokens, household_id, membership_id })))
}
