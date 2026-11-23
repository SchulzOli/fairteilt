//! Points ledger, accounts with streak and history (M3, KONZEPT §4.4, §6).
//!
//! The ledger is append-only: sums over entries, corrections are counter-entries. XP
//! (`contribution`) measure work and only come from tasks. Guthaben (`balance`) is no longer
//! earned or moved (since 23.11.2026); old entries stay in the books.

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{DateTime, NaiveDate, Utc};
use punkte_core::{LedgerDelta, LedgerKind, Streak, streak};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::api::households::membership;
use crate::api::tasks::household;
use crate::inbox::{self, Category, Note};
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

// ---------------------------------------------------------------- accounts

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct AccountDto {
    pub membership_id: Uuid,
    /// Guthaben (G) from before 23.11.2026; no longer earned or shown.
    pub balance: i64,
    /// XP earned in the current month (the period until settlement arrives in M4).
    pub xp_month: i64,
    pub xp_total: i64,
    pub streak: Streak,
    /// Own tasks waiting for someone to confirm them.
    pub waiting: i64,
    /// Bonus XP this member can still give away this month (household rule `bonus_xp_per_month`).
    pub bonus_left: i64,
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
        bonus_given: i64,
    }
    let rows = sqlx::query_as::<_, Row>(
        "SELECT m.id,
                COALESCE(SUM(l.balance_delta), 0)::BIGINT AS balance,
                COALESCE(SUM(l.contribution_delta) FILTER (WHERE l.created_at >= $2), 0)::BIGINT AS xp_month,
                COALESCE(SUM(l.contribution_delta), 0)::BIGINT AS xp_total,
                (SELECT COUNT(*) FROM tasks t WHERE t.completed_by = m.id AND t.status = 'submitted') AS waiting,
                (SELECT COALESCE(SUM(b.amount), 0) FROM bounties b
                 WHERE b.pledger_id = m.id AND b.created_at >= $3 AND b.status <> 'refunded')::BIGINT AS bonus_given
         FROM memberships m LEFT JOIN ledger_entries l ON l.member_id = m.id
         WHERE m.household_id = $1 AND m.active
         GROUP BY m.id, m.seq ORDER BY m.seq",
    )
    .bind(household_id)
    .bind(crate::period::period_start(&state.db, household_id).await?)
    .bind(crate::api::tasks::month_start(hh.tz, now))
    .fetch_all(&state.db)
    .await?;

    // Streak: local days with at least one credited task (the day the work was done), for
    // everyone who shared it. A credit an admin took back does not count.
    let days: Vec<(Uuid, NaiveDate)> = sqlx::query_as(
        "SELECT DISTINCT r.member_id, (t.completed_at AT TIME ZONE $2)::date
         FROM ledger_entries r JOIN tasks t ON r.ref_type = 'task' AND t.id = r.ref_id
         WHERE r.household_id = $1 AND r.kind = 'task_reward' AND t.status = 'completed' AND t.completed_at > now() - interval '400 days'
           AND NOT EXISTS (SELECT 1 FROM ledger_entries c WHERE c.ref_type = 'ledger' AND c.ref_id = r.id)",
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
                    bonus_left: (hh.policy.bonus_xp_per_month - r.bonus_given).max(0),
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
    /// Title of the task the entry belongs to.
    pub task_title: Option<String>,
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
        "SELECT l.id, l.kind, l.contribution_delta, l.balance_delta, l.note, l.created_at, t.title AS task_title, l.created_by,
                EXISTS (SELECT 1 FROM ledger_entries c WHERE c.ref_type = 'ledger' AND c.ref_id = l.id) AS reversed
         FROM ledger_entries l
         LEFT JOIN ledger_entries orig ON l.ref_type = 'ledger' AND orig.id = l.ref_id
         LEFT JOIN tasks t ON t.id = CASE WHEN l.ref_type = 'task' THEN l.ref_id WHEN orig.ref_type = 'task' THEN orig.ref_id END
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
        title: Option<String>,
    }
    let entry = sqlx::query_as::<_, Entry>(
        "SELECT l.member_id, l.kind, l.contribution_delta, t.title
         FROM ledger_entries l LEFT JOIN tasks t ON l.ref_type = 'task' AND t.id = l.ref_id
         WHERE l.id = $1 AND l.household_id = $2 FOR UPDATE OF l",
    )
    .bind(entry_id)
    .bind(household_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Entry {
        member_id: member,
        kind,
        contribution_delta: xp,
        title,
    } = entry.ok_or(AppError::NotFound)?;
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
    // XP only: Guthaben that older credits carried stays (it is no longer shown).
    let delta = LedgerDelta {
        kind: LedgerKind::ManualAdjust,
        contribution: -xp,
        balance: 0,
    };
    Ledger {
        household: household_id,
        by: me,
    }
    .book(&mut tx, member, delta, "ledger", entry_id, Some(note))
    .await?;
    let who = inbox::name(&mut tx, me).await?;
    inbox::add(
        &mut tx,
        Note::new(member, "reversed", Category::Points, format!("„{}“ zurückgenommen", title.as_deref().unwrap_or("Aufgabe")))
            .body(format!("„{note}“ – {who}"))
            .about("ledger", entry_id),
    )
    .await?;
    tx.commit().await?;
    state.live.notify(household_id, r#"{"type":"points_changed"}"#);
    state.live.notify_json(
        household_id,
        serde_json::json!({ "type": "reversed", "member": member, "by": me, "task": title, "xp": xp, "note": note }),
    );
    Ok(Json(ReverseResponse { xp }))
}
