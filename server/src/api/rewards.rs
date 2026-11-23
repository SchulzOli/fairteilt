//! Reward catalog (KONZEPT §6): things you "buy" with G ("Film aussuchen", 30 G).
//! Optional approval, stock and cooldown. The G are taken when redeeming; a rejected
//! redemption gets them back.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Duration, Utc};
use punkte_core::{LedgerDelta, LedgerKind};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::households::membership;
use crate::api::points::{Ledger, balance, lock_member};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::AppState;

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct RewardDto {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub cost: i64,
    pub needs_approval: bool,
    /// Left in stock (None = unlimited).
    pub left: Option<i64>,
    pub cooldown_hours: Option<i32>,
    /// For you: redeemable again from (cooldown), None = now.
    pub available_at: Option<DateTime<Utc>>,
}

#[derive(sqlx::FromRow)]
struct RewardRow {
    id: Uuid,
    title: String,
    description: Option<String>,
    cost: i64,
    needs_approval: bool,
    stock: Option<i32>,
    cooldown_hours: Option<i32>,
    used: i64,
    my_last: Option<DateTime<Utc>>,
}

const REWARD_QUERY: &str = "SELECT r.id, r.title, r.description, r.cost, r.needs_approval, r.stock, r.cooldown_hours,
        (SELECT COUNT(*) FROM reward_redemptions x WHERE x.reward_id = r.id AND x.status <> 'rejected') AS used,
        (SELECT MAX(x.created_at) FROM reward_redemptions x WHERE x.reward_id = r.id AND x.member_id = $2 AND x.status <> 'rejected') AS my_last
     FROM rewards r";

fn to_dto(r: RewardRow) -> RewardDto {
    let available_at = match (r.cooldown_hours, r.my_last) {
        (Some(h), Some(last)) if last + Duration::hours(i64::from(h)) > Utc::now() => Some(last + Duration::hours(i64::from(h))),
        _ => None,
    };
    RewardDto {
        id: r.id,
        title: r.title,
        description: r.description,
        cost: r.cost,
        needs_approval: r.needs_approval,
        left: r.stock.map(|s| (i64::from(s) - r.used).max(0)),
        cooldown_hours: r.cooldown_hours,
        available_at,
    }
}

pub async fn list(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<Vec<RewardDto>>> {
    let (me, _) = membership(&state, user.user_id, household_id).await?;
    let rows = sqlx::query_as::<_, RewardRow>(sqlx::AssertSqlSafe(format!(
        "{REWARD_QUERY} WHERE r.household_id = $1 AND NOT r.archived ORDER BY r.cost, r.title"
    )))
    .bind(household_id)
    .bind(me)
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows.into_iter().map(to_dto).collect()))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct RewardRequest {
    pub title: String,
    pub description: Option<String>,
    pub cost: i64,
    pub needs_approval: bool,
    pub stock: Option<i32>,
    pub cooldown_hours: Option<i32>,
}

fn check(req: &RewardRequest) -> AppResult<(String, Option<String>)> {
    let title = req.title.trim();
    if title.is_empty() || title.chars().count() > 60 {
        return Err(AppError::BadRequest("Der Name muss 1 bis 60 Zeichen lang sein.".into()));
    }
    if !(1..=100_000).contains(&req.cost) {
        return Err(AppError::BadRequest("Der Preis liegt zwischen 1 und 100 000 G.".into()));
    }
    if req.stock.is_some_and(|s| s < 0) || req.cooldown_hours.is_some_and(|h| !(1..=24 * 365).contains(&h)) {
        return Err(AppError::BadRequest("Bestand ab 0, Abkühlzeit 1 Stunde bis 1 Jahr.".into()));
    }
    let description = req.description.as_deref().map(str::trim).filter(|d| !d.is_empty()).map(str::to_string);
    if description.as_ref().is_some_and(|d| d.chars().count() > 200) {
        return Err(AppError::BadRequest("Die Beschreibung darf höchstens 200 Zeichen lang sein.".into()));
    }
    Ok((title.to_string(), description))
}

async fn admin(state: &AppState, user: &AuthUser, household_id: Uuid) -> AppResult<Uuid> {
    let (me, role) = membership(state, user.user_id, household_id).await?;
    if role != "admin" {
        return Err(AppError::Forbidden("Belohnungen verwalten Admins.".into()));
    }
    Ok(me)
}

pub async fn create(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<RewardRequest>,
) -> AppResult<(StatusCode, Json<RewardDto>)> {
    let me = admin(&state, &user, household_id).await?;
    let (title, description) = check(&req)?;
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO rewards (id, household_id, title, description, cost, needs_approval, stock, cooldown_hours)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(household_id)
    .bind(title)
    .bind(description)
    .bind(req.cost)
    .bind(req.needs_approval)
    .bind(req.stock)
    .bind(req.cooldown_hours)
    .execute(&state.db)
    .await?;
    state.live.notify(household_id, r#"{"type":"rewards_changed"}"#);
    let row = sqlx::query_as::<_, RewardRow>(sqlx::AssertSqlSafe(format!("{REWARD_QUERY} WHERE r.id = $1")))
        .bind(id)
        .bind(me)
        .fetch_one(&state.db)
        .await?;
    Ok((StatusCode::CREATED, Json(to_dto(row))))
}

pub async fn update(
    State(state): State<AppState>,
    user: AuthUser,
    Path((household_id, reward_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<RewardRequest>,
) -> AppResult<StatusCode> {
    admin(&state, &user, household_id).await?;
    let (title, description) = check(&req)?;
    let r = sqlx::query(
        "UPDATE rewards SET title = $3, description = $4, cost = $5, needs_approval = $6, stock = $7, cooldown_hours = $8
         WHERE id = $1 AND household_id = $2 AND NOT archived",
    )
    .bind(reward_id)
    .bind(household_id)
    .bind(title)
    .bind(description)
    .bind(req.cost)
    .bind(req.needs_approval)
    .bind(req.stock)
    .bind(req.cooldown_hours)
    .execute(&state.db)
    .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    state.live.notify(household_id, r#"{"type":"rewards_changed"}"#);
    Ok(StatusCode::NO_CONTENT)
}

/// Archived, not deleted: past redemptions keep their reward.
pub async fn archive(State(state): State<AppState>, user: AuthUser, Path((household_id, reward_id)): Path<(Uuid, Uuid)>) -> AppResult<StatusCode> {
    admin(&state, &user, household_id).await?;
    let r = sqlx::query("UPDATE rewards SET archived = true WHERE id = $1 AND household_id = $2").bind(reward_id).bind(household_id).execute(&state.db).await?;
    if r.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    state.live.notify(household_id, r#"{"type":"rewards_changed"}"#);
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- redemptions

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, sqlx::FromRow)]
pub struct RedemptionDto {
    pub id: Uuid,
    pub reward_id: Uuid,
    pub title: String,
    pub member_id: Uuid,
    pub cost: i64,
    /// `requested` (waits for an admin), `approved` or `rejected` (G returned).
    pub status: String,
    pub created_at: DateTime<Utc>,
}

const REDEMPTION_COLUMNS: &str = "x.id, x.reward_id, r.title, x.member_id, x.cost, x.status, x.created_at";

async fn load(state: &AppState, id: Uuid) -> AppResult<RedemptionDto> {
    Ok(sqlx::query_as::<_, RedemptionDto>(sqlx::AssertSqlSafe(format!(
        "SELECT {REDEMPTION_COLUMNS} FROM reward_redemptions x JOIN rewards r ON r.id = x.reward_id WHERE x.id = $1"
    )))
    .bind(id)
    .fetch_one(&state.db)
    .await?)
}

pub async fn redeem(State(state): State<AppState>, user: AuthUser, Path(reward_id): Path<Uuid>) -> AppResult<(StatusCode, Json<RedemptionDto>)> {
    let household_id: Uuid = sqlx::query_scalar("SELECT household_id FROM rewards WHERE id = $1 AND NOT archived")
        .bind(reward_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let (me, _) = membership(&state, user.user_id, household_id).await?;
    let mut tx = state.db.begin().await?;
    // Lock the reward (stock) and the member (balance) for this redemption.
    let (cost, needs_approval, stock, cooldown): (i64, bool, Option<i32>, Option<i32>) =
        sqlx::query_as("SELECT cost, needs_approval, stock, cooldown_hours FROM rewards WHERE id = $1 FOR UPDATE")
            .bind(reward_id)
            .fetch_one(&mut *tx)
            .await?;
    if let Some(stock) = stock {
        let used: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM reward_redemptions WHERE reward_id = $1 AND status <> 'rejected'")
            .bind(reward_id)
            .fetch_one(&mut *tx)
            .await?;
        if used >= i64::from(stock) {
            return Err(AppError::Conflict("Diese Belohnung ist gerade vergriffen.".into()));
        }
    }
    if let Some(hours) = cooldown {
        let last: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT MAX(created_at) FROM reward_redemptions WHERE reward_id = $1 AND member_id = $2 AND status <> 'rejected'",
        )
        .bind(reward_id)
        .bind(me)
        .fetch_one(&mut *tx)
        .await?;
        if let Some(last) = last
            && last + Duration::hours(i64::from(hours)) > Utc::now()
        {
            return Err(AppError::Conflict("Diese Belohnung gibt es für dich erst später wieder.".into()));
        }
    }
    lock_member(&mut tx, me).await?;
    let have = balance(&mut tx, me).await?;
    if have < cost {
        return Err(AppError::BadRequest(format!("Nicht genug Guthaben: {have} G vorhanden, {cost} G nötig.")));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO reward_redemptions (id, reward_id, member_id, cost, status) VALUES ($1, $2, $3, $4, $5)")
        .bind(id)
        .bind(reward_id)
        .bind(me)
        .bind(cost)
        .bind(if needs_approval { "requested" } else { "approved" })
        .execute(&mut *tx)
        .await?;
    let delta = LedgerDelta { kind: LedgerKind::RewardRedeem, contribution: 0, balance: -cost };
    Ledger { household: household_id, by: me }.book(&mut tx, me, delta, "redemption", id, None).await?;
    tx.commit().await?;
    state.live.notify(household_id, r#"{"type":"points_changed"}"#);
    state.live.notify(household_id, r#"{"type":"rewards_changed"}"#);
    Ok((StatusCode::CREATED, Json(load(&state, id).await?)))
}

/// Your redemptions; admins also see everything waiting for a decision.
pub async fn redemptions(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<Vec<RedemptionDto>>> {
    let (me, role) = membership(&state, user.user_id, household_id).await?;
    let rows = sqlx::query_as::<_, RedemptionDto>(sqlx::AssertSqlSafe(format!(
        "SELECT {REDEMPTION_COLUMNS} FROM reward_redemptions x JOIN rewards r ON r.id = x.reward_id
         WHERE r.household_id = $1 AND (x.member_id = $2 OR ($3 AND x.status = 'requested'))
         ORDER BY x.created_at DESC LIMIT 40"
    )))
    .bind(household_id)
    .bind(me)
    .bind(role == "admin")
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct DecideRedemptionRequest {
    pub approve: bool,
}

/// Admins approve a requested redemption or reject it (the G go back). Not your own.
pub async fn decide(
    State(state): State<AppState>,
    user: AuthUser,
    Path(redemption_id): Path<Uuid>,
    Json(req): Json<DecideRedemptionRequest>,
) -> AppResult<Json<RedemptionDto>> {
    let (household_id, member): (Uuid, Uuid) = sqlx::query_as(
        "SELECT r.household_id, x.member_id FROM reward_redemptions x JOIN rewards r ON r.id = x.reward_id WHERE x.id = $1",
    )
    .bind(redemption_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;
    let (me, role) = membership(&state, user.user_id, household_id).await?;
    if role != "admin" {
        return Err(AppError::Forbidden("Belohnungen geben Admins frei.".into()));
    }
    if member == me {
        return Err(AppError::Forbidden("Die eigene Belohnung gibt bitte jemand anderes frei.".into()));
    }
    let mut tx = state.db.begin().await?;
    let (status, cost): (String, i64) = sqlx::query_as("SELECT status, cost FROM reward_redemptions WHERE id = $1 FOR UPDATE")
        .bind(redemption_id)
        .fetch_one(&mut *tx)
        .await?;
    if status != "requested" {
        return Err(AppError::Conflict("Darüber ist schon entschieden.".into()));
    }
    sqlx::query("UPDATE reward_redemptions SET status = $2, decided_by = $3, decided_at = now() WHERE id = $1")
        .bind(redemption_id)
        .bind(if req.approve { "approved" } else { "rejected" })
        .bind(me)
        .execute(&mut *tx)
        .await?;
    if !req.approve {
        let back = LedgerDelta { kind: LedgerKind::RewardRedeem, contribution: 0, balance: cost };
        Ledger { household: household_id, by: me }.book(&mut tx, member, back, "redemption", redemption_id, Some("zurück")).await?;
    }
    tx.commit().await?;
    let r = load(&state, redemption_id).await?;
    state.live.notify(household_id, r#"{"type":"points_changed"}"#);
    state.live.notify_json(
        household_id,
        serde_json::json!({
            "type": if req.approve { "redemption_approved" } else { "redemption_rejected" },
            "member": member, "by": me, "task": r.title, "g": cost,
        }),
    );
    Ok(Json(r))
}
