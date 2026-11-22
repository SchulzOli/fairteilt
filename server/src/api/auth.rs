//! Accounts and sessions: register, login, refresh (rotating), logout, me.

use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{
    ACCESS_TTL, AuthUser, REFRESH_TTL, dummy_hash, hash_password, hash_token, issue_access_token,
    new_refresh_token, verify_password,
};
use crate::error::{AppError, AppResult};
use crate::ratelimit::ClientKey;
use crate::AppState;

const MIN_PASSWORD_LEN: usize = 10;

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub display_name: String,
    pub device_name: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    pub device_name: Option<String>,
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Serialize)]
pub struct UserDto {
    pub id: Uuid,
    pub email: Option<String>,
    pub display_name: String,
}

#[derive(Serialize)]
pub struct TokenResponse {
    pub user: UserDto,
    pub access_token: String,
    pub refresh_token: String,
    /// Seconds until the access token expires.
    pub expires_in: u64,
}

fn normalize_email(email: &str) -> AppResult<String> {
    let e = email.trim().to_lowercase();
    let valid = e.len() <= 254
        && e.split_once('@').is_some_and(|(local, domain)| !local.is_empty() && domain.contains('.') && !domain.starts_with('.'));
    if !valid {
        return Err(AppError::BadRequest("Bitte gib eine gültige E-Mail-Adresse ein.".into()));
    }
    Ok(e)
}

pub(crate) fn device(name: Option<String>) -> String {
    let n = name.unwrap_or_default();
    let n = n.trim();
    if n.is_empty() { "Unbekanntes Gerät".into() } else { n.chars().take(60).collect() }
}

/// Creates a session for the user and returns fresh tokens.
pub(crate) async fn start_session(state: &AppState, user: UserDto, device_name: String) -> AppResult<TokenResponse> {
    let session_id = Uuid::new_v4();
    let (refresh_token, refresh_hash) = new_refresh_token();
    let expires_at = Utc::now() + REFRESH_TTL;
    sqlx::query("INSERT INTO sessions (id, user_id, device_name, refresh_hash, expires_at) VALUES ($1, $2, $3, $4, $5)")
        .bind(session_id)
        .bind(user.id)
        .bind(device_name)
        .bind(refresh_hash)
        .bind(expires_at)
        .execute(&state.db)
        .await?;
    let access_token = issue_access_token(&state.auth, user.id, session_id)?;
    Ok(TokenResponse { user, access_token, refresh_token, expires_in: ACCESS_TTL.as_secs() })
}

pub async fn register(
    State(state): State<AppState>,
    client: ClientKey,
    Json(req): Json<RegisterRequest>,
) -> AppResult<(StatusCode, Json<TokenResponse>)> {
    state.limiter.check(&format!("register:{}", client.0), 10, Duration::from_secs(3600))?;
    let email = normalize_email(&req.email)?;
    let name = req.display_name.trim();
    if name.is_empty() || name.chars().count() > 40 {
        return Err(AppError::BadRequest("Der Name muss 1 bis 40 Zeichen lang sein.".into()));
    }
    if req.password.chars().count() < MIN_PASSWORD_LEN {
        return Err(AppError::BadRequest(format!("Das Passwort braucht mindestens {MIN_PASSWORD_LEN} Zeichen.")));
    }
    let hash = hash_password(&req.password)?;
    let user_id = Uuid::new_v4();
    let inserted = sqlx::query(
        "INSERT INTO users (id, email, password_hash, display_name) VALUES ($1, $2, $3, $4) ON CONFLICT (email) DO NOTHING",
    )
    .bind(user_id)
    .bind(&email)
    .bind(hash)
    .bind(name)
    .execute(&state.db)
    .await?;
    if inserted.rows_affected() == 0 {
        return Err(AppError::Conflict("Für diese E-Mail-Adresse gibt es schon ein Konto. Melde dich stattdessen an.".into()));
    }
    let user = UserDto { id: user_id, email: Some(email), display_name: name.to_string() };
    let tokens = start_session(&state, user, device(req.device_name)).await?;
    Ok((StatusCode::CREATED, Json(tokens)))
}

pub async fn login(
    State(state): State<AppState>,
    client: ClientKey,
    Json(req): Json<LoginRequest>,
) -> AppResult<Json<TokenResponse>> {
    let wrong = || AppError::BadRequest("E-Mail-Adresse oder Passwort stimmt nicht.".into());
    let email = req.email.trim().to_lowercase();
    // Per address (slows guessing one account) and per client (slows spraying many).
    state.limiter.check(&format!("login:{}:{}", client.0, email), 10, Duration::from_secs(900))?;
    state.limiter.check(&format!("login:{}", client.0), 40, Duration::from_secs(900))?;
    let row: Option<(Uuid, Option<String>, String)> =
        sqlx::query_as("SELECT id, password_hash, display_name FROM users WHERE email = $1")
            .bind(&email)
            .fetch_optional(&state.db)
            .await?;
    // Always run one Argon2 verification so timing does not reveal whether the account exists.
    let (user_id, hash, name) = match row {
        Some((id, Some(hash), name)) => (Some(id), hash, name),
        _ => (None, dummy_hash().to_string(), String::new()),
    };
    let ok = verify_password(&req.password, &hash);
    let user_id = match (user_id, ok) {
        (Some(id), true) => id,
        _ => return Err(wrong()),
    };
    let user = UserDto { id: user_id, email: Some(email), display_name: name };
    Ok(Json(start_session(&state, user, device(req.device_name)).await?))
}

/// Rotates the refresh token: the old one stops working, the session lives on.
pub async fn refresh(State(state): State<AppState>, Json(req): Json<RefreshRequest>) -> AppResult<Json<TokenResponse>> {
    let (new_token, new_hash) = new_refresh_token();
    let expires_at = Utc::now() + REFRESH_TTL;
    let row: Option<(Uuid, Uuid)> = sqlx::query_as(
        "UPDATE sessions SET refresh_hash = $2, expires_at = $3, last_used_at = now()
         WHERE refresh_hash = $1 AND revoked_at IS NULL AND expires_at > now()
         RETURNING id, user_id",
    )
    .bind(hash_token(req.refresh_token.trim()))
    .bind(new_hash)
    .bind(expires_at)
    .fetch_optional(&state.db)
    .await?;
    let (session_id, user_id) = row.ok_or(AppError::Unauthorized)?;
    let user = load_user(&state, user_id).await?;
    let access_token = issue_access_token(&state.auth, user_id, session_id)?;
    Ok(Json(TokenResponse { user, access_token, refresh_token: new_token, expires_in: ACCESS_TTL.as_secs() }))
}

pub async fn logout(State(state): State<AppState>, user: AuthUser) -> AppResult<StatusCode> {
    sqlx::query("UPDATE sessions SET revoked_at = now() WHERE id = $1 AND user_id = $2")
        .bind(user.session_id)
        .bind(user.user_id)
        .execute(&state.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn load_user(state: &AppState, user_id: Uuid) -> AppResult<UserDto> {
    let (id, email, display_name): (Uuid, Option<String>, String) =
        sqlx::query_as("SELECT id, email, display_name FROM users WHERE id = $1")
            .bind(user_id)
            .fetch_optional(&state.db)
            .await?
            .ok_or(AppError::Unauthorized)?;
    Ok(UserDto { id, email, display_name })
}

#[derive(Serialize, sqlx::FromRow)]
pub struct MembershipDto {
    pub membership_id: Uuid,
    pub household_id: Uuid,
    pub household_name: String,
    pub role: String,
    pub color: String,
    pub animal: String,
    pub weight: f64,
    pub joined_at: DateTime<Utc>,
}

#[derive(Serialize)]
pub struct MeResponse {
    pub user: UserDto,
    pub memberships: Vec<MembershipDto>,
}

pub async fn me(State(state): State<AppState>, user: AuthUser) -> AppResult<Json<MeResponse>> {
    let u = load_user(&state, user.user_id).await?;
    let memberships = sqlx::query_as::<_, MembershipDto>(
        "SELECT m.id AS membership_id, h.id AS household_id, h.name AS household_name, m.role, m.color, m.animal,
                m.weight, m.joined_at
         FROM memberships m JOIN households h ON h.id = m.household_id
         WHERE m.user_id = $1 AND m.active
         ORDER BY m.joined_at",
    )
    .bind(user.user_id)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(MeResponse { user: u, memberships }))
}

