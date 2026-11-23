//! Points ledger, accounts with streak, history and transfers (M3, KONZEPT §4.4, §4.5, §6).
//!
//! The ledger is append-only: balances are sums over entries, corrections are counter-entries.
//! XP (`contribution`) measure work and only come from tasks; G (`balance`) can move freely.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, NaiveDate, Utc};
use punkte_core::{LedgerDelta, LedgerKind, Streak, streak};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::api::households::membership;
use crate::api::tasks::household;
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::AppState;

// ---------------------------------------------------------------- ledger helpers

/// Books entries for one household, written by `by`.
pub(crate) struct Ledger {
    pub household: Uuid,
    pub by: Uuid,
}

impl Ledger {
    pub async fn book(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        member: Uuid,
        delta: LedgerDelta,
        ref_type: &str,
        ref_id: Uuid,
        note: Option<&str>,
    ) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO ledger_entries (id, household_id, member_id, kind, contribution_delta, balance_delta, ref_type, ref_id, note, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
        )
        .bind(Uuid::new_v4())
        .bind(self.household)
        .bind(member)
        .bind(delta.kind.as_str())
        .bind(delta.contribution)
        .bind(delta.balance)
        .bind(ref_type)
        .bind(ref_id)
        .bind(note)
        .bind(self.by)
        .execute(&mut **tx)
        .await?;
        Ok(())
    }
}

/// Serializes balance checks of one member (pledges, transfers) until the transaction ends.
pub(crate) async fn lock_member(tx: &mut Transaction<'_, Postgres>, member: Uuid) -> AppResult<()> {
    sqlx::query("SELECT id FROM memberships WHERE id = $1 FOR UPDATE").bind(member).fetch_one(&mut **tx).await?;
    Ok(())
}

pub(crate) async fn balance(tx: &mut Transaction<'_, Postgres>, member: Uuid) -> AppResult<i64> {
    Ok(sqlx::query_scalar("SELECT COALESCE(SUM(balance_delta), 0)::BIGINT FROM ledger_entries WHERE member_id = $1")
        .bind(member)
        .fetch_one(&mut **tx)
        .await?)
}

// ---------------------------------------------------------------- accounts

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct AccountDto {
    pub membership_id: Uuid,
    /// Guthaben (G): transferable, for bonuses and rewards.
    pub balance: i64,
    /// XP earned in the current month (the period until settlement arrives in M4).
    pub xp_month: i64,
    pub xp_total: i64,
    pub streak: Streak,
    /// Own tasks waiting for someone to confirm them.
    pub waiting: i64,
}

/// Accounts of all active members (in member order, no ranking).
pub async fn accounts(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<Vec<AccountDto>>> {
    membership(&state, user.user_id, household_id).await?;
    let hh = household(&state.db, household_id).await?;
    let now = Utc::now();
    #[derive(sqlx::FromRow)]
    struct Row {
        id: Uuid,
        balance: i64,
        xp_month: i64,
        xp_total: i64,
        waiting: i64,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT m.id,
                COALESCE(SUM(l.balance_delta), 0)::BIGINT AS balance,
                COALESCE(SUM(l.contribution_delta) FILTER (WHERE l.created_at >= $2), 0)::BIGINT AS xp_month,
                COALESCE(SUM(l.contribution_delta), 0)::BIGINT AS xp_total,
                (SELECT COUNT(*) FROM tasks t WHERE t.completed_by = m.id AND t.status = 'submitted') AS waiting
         FROM memberships m LEFT JOIN ledger_entries l ON l.member_id = m.id
         WHERE m.household_id = $1 AND m.active
         GROUP BY m.id, m.seq ORDER BY m.seq",
    )
    .bind(household_id)
    .bind(crate::period::period_start(&state.db, household_id).await?)
    .fetch_all(&state.db)
    .await?;

    // Streak: local days with at least one credited task (the day the work was done).
    // A credit an admin took back does not count.
    let days: Vec<(Uuid, NaiveDate)> = sqlx::query_as(
        "SELECT DISTINCT completed_by, (completed_at AT TIME ZONE $2)::date FROM tasks
         WHERE household_id = $1 AND status = 'completed' AND completed_by IS NOT NULL AND completed_at > now() - interval '400 days'
           AND NOT EXISTS (SELECT 1 FROM ledger_entries r JOIN ledger_entries c ON c.ref_type = 'ledger' AND c.ref_id = r.id
                           WHERE r.ref_type = 'task' AND r.ref_id = tasks.id AND r.kind = 'task_reward')",
    )
    .bind(household_id)
    .bind(hh.tz.name())
    .fetch_all(&state.db)
    .await?;
    let today = now.with_timezone(&hh.tz).date_naive();

    Ok(Json(
        rows.into_iter()
            .map(|r| {
                let mine: Vec<NaiveDate> = days.iter().filter(|(m, _)| *m == r.id).map(|(_, d)| *d).collect();
                AccountDto {
                    membership_id: r.id,
                    balance: r.balance,
                    xp_month: r.xp_month,
                    xp_total: r.xp_total,
                    streak: streak(&mine, today),
                    waiting: r.waiting,
                }
            })
            .collect(),
    ))
}

// ---------------------------------------------------------------- history

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, sqlx::FromRow)]
pub struct LedgerEntryDto {
    pub id: Uuid,
    pub kind: String,
    pub contribution_delta: i64,
    pub balance_delta: i64,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    /// Task title (task entries) or reward title (redemptions).
    pub task_title: Option<String>,
    /// The other person of a transfer.
    pub counterpart_id: Option<Uuid>,
    /// Who booked it (the reviewer for task rewards after approval, the admin for corrections).
    pub created_by: Option<Uuid>,
    /// A correction already took this entry back.
    pub reversed: bool,
}

#[derive(Deserialize)]
pub struct HistoryQuery {
    /// Whose account (default: your own; others only for admins).
    pub member: Option<Uuid>,
    /// Entries older than this (paging).
    pub before: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

pub async fn history(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Query(q): Query<HistoryQuery>,
) -> AppResult<Json<Vec<LedgerEntryDto>>> {
    let (me, role) = membership(&state, user.user_id, household_id).await?;
    let member = q.member.unwrap_or(me);
    if member != me && role != "admin" {
        return Err(AppError::Forbidden("Den Verlauf anderer sehen nur Admins.".into()));
    }
    let rows = sqlx::query_as::<_, LedgerEntryDto>(
        "SELECT l.id, l.kind, l.contribution_delta, l.balance_delta, COALESCE(l.note, tr.note) AS note, l.created_at,
                COALESCE(t.title, rw.title) AS task_title,
                CASE WHEN l.kind = 'transfer_out' THEN tr.to_id WHEN l.kind = 'transfer_in' THEN tr.from_id END AS counterpart_id,
                l.created_by,
                EXISTS (SELECT 1 FROM ledger_entries c WHERE c.ref_type = 'ledger' AND c.ref_id = l.id) AS reversed
         FROM ledger_entries l
         LEFT JOIN ledger_entries orig ON l.ref_type = 'ledger' AND orig.id = l.ref_id
         LEFT JOIN tasks t ON t.id = CASE WHEN l.ref_type = 'task' THEN l.ref_id WHEN orig.ref_type = 'task' THEN orig.ref_id END
         LEFT JOIN transfers tr ON l.ref_type = 'transfer' AND tr.id = l.ref_id
         LEFT JOIN reward_redemptions rr ON l.ref_type = 'redemption' AND rr.id = l.ref_id
         LEFT JOIN rewards rw ON rw.id = rr.reward_id
         WHERE l.household_id = $1 AND l.member_id = $2 AND ($3::timestamptz IS NULL OR l.created_at < $3)
         ORDER BY l.created_at DESC, l.id LIMIT $4",
    )
    .bind(household_id)
    .bind(member)
    .bind(q.before)
    .bind(q.limit.unwrap_or(30).clamp(1, 100))
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

// ---------------------------------------------------------------- corrections

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct ReverseRequest {
    /// Why ("War doch nicht fertig"). Shown to the person in their history.
    pub note: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct ReverseResponse {
    pub xp: i64,
    /// G taken back; less than credited when the person already spent some (G never go negative).
    pub g: i64,
}

/// Admins take back a task credit with a counter-entry (KONZEPT §4.5: never edit, always book against).
/// Bonuses stay paid out: they were someone's gift for the work.
pub async fn reverse(
    State(state): State<AppState>,
    user: AuthUser,
    Path((household_id, entry_id)): Path<(Uuid, Uuid)>,
    Json(req): Json<ReverseRequest>,
) -> AppResult<Json<ReverseResponse>> {
    let (me, role) = membership(&state, user.user_id, household_id).await?;
    if role != "admin" {
        return Err(AppError::Forbidden("Buchungen zurücknehmen können nur Admins.".into()));
    }
    let note = req.note.trim();
    if note.is_empty() || note.chars().count() > 200 {
        return Err(AppError::BadRequest("Bitte kurz begründen (bis 200 Zeichen).".into()));
    }
    let mut tx = state.db.begin().await?;
    #[derive(sqlx::FromRow)]
    struct Entry {
        member_id: Uuid,
        kind: String,
        contribution_delta: i64,
        balance_delta: i64,
        title: Option<String>,
    }
    let entry = sqlx::query_as::<_, Entry>(
        "SELECT l.member_id, l.kind, l.contribution_delta, l.balance_delta, t.title
         FROM ledger_entries l LEFT JOIN tasks t ON l.ref_type = 'task' AND t.id = l.ref_id
         WHERE l.id = $1 AND l.household_id = $2 FOR UPDATE OF l",
    )
    .bind(entry_id)
    .bind(household_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Entry { member_id: member, kind, contribution_delta: xp, balance_delta: g, title } = entry.ok_or(AppError::NotFound)?;
    if kind != LedgerKind::TaskReward.as_str() {
        return Err(AppError::BadRequest("Zurücknehmen geht nur bei Gutschriften für Aufgaben.".into()));
    }
    if member == me {
        return Err(AppError::Forbidden("Die eigene Gutschrift nimmt bitte jemand anderes zurück.".into()));
    }
    let already: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM ledger_entries WHERE ref_type = 'ledger' AND ref_id = $1)")
        .bind(entry_id)
        .fetch_one(&mut *tx)
        .await?;
    if already {
        return Err(AppError::Conflict("Diese Gutschrift wurde schon zurückgenommen.".into()));
    }
    lock_member(&mut tx, member).await?;
    let have = balance(&mut tx, member).await?;
    let g_back = g.min(have.max(0));
    let delta = LedgerDelta { kind: LedgerKind::ManualAdjust, contribution: -xp, balance: -g_back };
    Ledger { household: household_id, by: me }.book(&mut tx, member, delta, "ledger", entry_id, Some(note)).await?;
    tx.commit().await?;
    state.live.notify(household_id, r#"{"type":"points_changed"}"#);
    state.live.notify_json(
        household_id,
        serde_json::json!({ "type": "reversed", "member": member, "by": me, "task": title, "xp": xp, "g": g_back, "note": note }),
    );
    Ok(Json(ReverseResponse { xp, g: g_back }))
}

// ---------------------------------------------------------------- transfers

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct TransferRequest {
    pub to: Uuid,
    pub amount: i64,
    /// Always with a note ("Danke fürs Müll-Rausbringen").
    pub note: String,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize, sqlx::FromRow)]
pub struct TransferDto {
    pub id: Uuid,
    pub from_id: Uuid,
    pub to_id: Uuid,
    pub amount: i64,
    pub note: String,
    /// `done`, `pending` (a child's transfer waiting for an admin) or `declined`.
    pub status: String,
    pub created_at: DateTime<Utc>,
}

macro_rules! transfer_columns {
    () => {
        "id, from_id, to_id, amount, note, status, created_at"
    };
}

/// G sent (or waiting to be sent) by `member` this month.
async fn sent_this_period(tx: &mut Transaction<'_, Postgres>, member: Uuid, since: DateTime<Utc>, except: Option<Uuid>) -> AppResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount), 0)::BIGINT FROM transfers
         WHERE from_id = $1 AND status IN ('done', 'pending') AND created_at >= $2 AND ($3::uuid IS NULL OR id <> $3)",
    )
    .bind(member)
    .bind(since)
    .bind(except)
    .fetch_one(&mut **tx)
    .await?)
}

/// Personal event for the receiver of a transfer.
fn received(t: &TransferDto) -> serde_json::Value {
    serde_json::json!({ "type": "received", "member": t.to_id, "by": t.from_id, "g": t.amount, "note": t.note })
}

async fn execute(tx: &mut Transaction<'_, Postgres>, household: Uuid, t: &TransferDto, by: Uuid) -> AppResult<()> {
    let (out, inn) = LedgerDelta::transfer(t.amount);
    let ledger = Ledger { household, by };
    ledger.book(tx, t.from_id, out, "transfer", t.id, None).await?;
    ledger.book(tx, t.to_id, inn, "transfer", t.id, None).await?;
    Ok(())
}

pub async fn create_transfer(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<TransferRequest>,
) -> AppResult<(StatusCode, Json<TransferDto>)> {
    let (me, role) = membership(&state, user.user_id, household_id).await?;
    let hh = household(&state.db, household_id).await?;
    let note = req.note.trim();
    if note.is_empty() || note.chars().count() > 200 {
        return Err(AppError::BadRequest("Bitte schreib kurz dazu, wofür (bis 200 Zeichen).".into()));
    }
    if req.to == me {
        return Err(AppError::BadRequest("An dich selbst kannst du nichts übertragen.".into()));
    }
    let mut tx = state.db.begin().await?;
    let to_ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM memberships WHERE id = $1 AND household_id = $2 AND active)")
        .bind(req.to)
        .bind(household_id)
        .fetch_one(&mut *tx)
        .await?;
    if !to_ok {
        return Err(AppError::BadRequest("Diese Person ist nicht im Haushalt.".into()));
    }
    lock_member(&mut tx, me).await?;
    let have = balance(&mut tx, me).await?;
    let sent = sent_this_period(&mut tx, me, crate::period::period_start(&state.db, household_id).await?, None).await?;
    hh.policy.check_transfer(req.amount, have, sent).map_err(|e| AppError::BadRequest(e.to_string()))?;

    let pending = role == "child" && hh.policy.child_transfer_needs_approval;
    let transfer = sqlx::query_as::<_, TransferDto>(
        "INSERT INTO transfers (id, household_id, from_id, to_id, amount, note, status) VALUES ($1, $2, $3, $4, $5, $6, $7)
         RETURNING id, from_id, to_id, amount, note, status, created_at",
    )
    .bind(Uuid::new_v4())
    .bind(household_id)
    .bind(me)
    .bind(req.to)
    .bind(req.amount)
    .bind(note)
    .bind(if pending { "pending" } else { "done" })
    .fetch_one(&mut *tx)
    .await?;
    if !pending {
        execute(&mut tx, household_id, &transfer, me).await?;
    }
    tx.commit().await?;
    state.live.notify(household_id, r#"{"type":"points_changed"}"#);
    if !pending {
        state.live.notify_json(household_id, received(&transfer));
    }
    Ok((StatusCode::CREATED, Json(transfer)))
}

/// Your own transfers; admins also see all transfers waiting for a decision.
pub async fn list_transfers(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<Vec<TransferDto>>> {
    let (me, role) = membership(&state, user.user_id, household_id).await?;
    let rows = sqlx::query_as::<_, TransferDto>(concat!(
        "SELECT ",
        transfer_columns!(),
        " FROM transfers WHERE household_id = $1 AND (from_id = $2 OR to_id = $2 OR ($3 AND status = 'pending'))
         ORDER BY created_at DESC LIMIT 50"
    ))
    .bind(household_id)
    .bind(me)
    .bind(role == "admin")
    .fetch_all(&state.db)
    .await?;
    Ok(Json(rows))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct DecideTransferRequest {
    pub approve: bool,
}

/// Admins release or decline a child's transfer.
pub async fn decide_transfer(
    State(state): State<AppState>,
    user: AuthUser,
    Path(transfer_id): Path<Uuid>,
    Json(req): Json<DecideTransferRequest>,
) -> AppResult<Json<TransferDto>> {
    let household_id: Uuid = sqlx::query_scalar("SELECT household_id FROM transfers WHERE id = $1")
        .bind(transfer_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let (me, role) = membership(&state, user.user_id, household_id).await?;
    if role != "admin" {
        return Err(AppError::Forbidden("Übertragungen von Kindern geben Admins frei.".into()));
    }
    let hh = household(&state.db, household_id).await?;
    let mut tx = state.db.begin().await?;
    let t = sqlx::query_as::<_, TransferDto>(concat!("SELECT ", transfer_columns!(), " FROM transfers WHERE id = $1 FOR UPDATE"))
        .bind(transfer_id)
        .fetch_one(&mut *tx)
        .await?;
    if t.status != "pending" {
        return Err(AppError::Conflict("Über diese Übertragung ist schon entschieden.".into()));
    }
    let status = if req.approve {
        lock_member(&mut tx, t.from_id).await?;
        let have = balance(&mut tx, t.from_id).await?;
        let sent = sent_this_period(&mut tx, t.from_id, crate::period::period_start(&state.db, household_id).await?, Some(t.id)).await?;
        hh.policy.check_transfer(t.amount, have, sent).map_err(|e| AppError::BadRequest(e.to_string()))?;
        execute(&mut tx, household_id, &t, me).await?;
        "done"
    } else {
        "declined"
    };
    let t = sqlx::query_as::<_, TransferDto>(concat!(
        "UPDATE transfers SET status = $2, decided_by = $3, decided_at = now() WHERE id = $1 RETURNING ",
        transfer_columns!()
    ))
    .bind(transfer_id)
    .bind(status)
    .bind(me)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    state.live.notify(household_id, r#"{"type":"points_changed"}"#);
    if t.status == "done" {
        state.live.notify_json(household_id, received(&t));
    } else {
        state.live.notify_json(
            household_id,
            serde_json::json!({ "type": "transfer_declined", "member": t.from_id, "by": me, "g": t.amount, "to": t.to_id }),
        );
    }
    Ok(Json(t))
}
