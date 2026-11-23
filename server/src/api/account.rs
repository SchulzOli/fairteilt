//! Your data (M7, DSGVO Art. 15/17/20): export, delete the account, delete a household,
//! remove members.
//!
//! Deleting an account keeps an anonymous row ("Gelöschtes Konto"): ledger, history and the
//! other members' settlements point to it. Everything personal (e-mail, password, name, PIN,
//! sessions, devices, inbox, preferences) is removed. A household where you are the only
//! member is deleted with you; one where others stay needs another admin first.

use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use serde::Deserialize;
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

use crate::api::households::membership;
use crate::auth::{AuthUser, verify_password};
use crate::error::{AppError, AppResult};
use crate::AppState;

async fn json_of(conn: &mut PgConnection, sql: &'static str, id: Uuid) -> AppResult<serde_json::Value> {
    Ok(sqlx::query_scalar::<_, Option<serde_json::Value>>(sql).bind(id).fetch_one(&mut *conn).await?.unwrap_or(serde_json::json!([])))
}

fn download(value: serde_json::Value, name: &str) -> AppResult<Response> {
    let body = serde_json::to_vec_pretty(&value).map_err(anyhow::Error::from)?;
    let mut res = (StatusCode::OK, body).into_response();
    res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let disposition = format!("attachment; filename=\"{name}-{}.json\"", Utc::now().format("%Y-%m-%d"));
    res.headers_mut().insert(header::CONTENT_DISPOSITION, HeaderValue::from_str(&disposition).map_err(anyhow::Error::from)?);
    Ok(res)
}

/// Everything about you: account, memberships, your bookings, transfers, rewards, inbox,
/// the tasks you did (DSGVO Art. 15 and 20, machine-readable).
pub async fn export_me(State(state): State<AppState>, user: AuthUser) -> AppResult<Response> {
    let mut conn = state.db.acquire().await?;
    let account: serde_json::Value = sqlx::query_scalar(
        "SELECT json_build_object('id', id, 'email', email, 'display_name', display_name, 'locale', locale, 'created_at', created_at)
         FROM users WHERE id = $1",
    )
    .bind(user.user_id)
    .fetch_one(&mut *conn)
    .await?;
    let memberships: Vec<(Uuid, Uuid)> =
        sqlx::query_as("SELECT id, household_id FROM memberships WHERE user_id = $1 ORDER BY joined_at").bind(user.user_id).fetch_all(&mut *conn).await?;
    let mut households = Vec::new();
    for (mid, hid) in memberships {
        households.push(serde_json::json!({
            "household": sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT json_build_object('id', id, 'name', name, 'timezone', timezone, 'created_at', created_at) FROM households WHERE id = $1",
            ).bind(hid).fetch_one(&mut *conn).await?,
            "membership": sqlx::query_scalar::<_, serde_json::Value>(
                "SELECT json_build_object('id', id, 'role', role, 'color', color, 'animal', animal, 'weight', weight, 'joined_at', joined_at,
                                          'active', active, 'has_pin', pin_hash IS NOT NULL)
                 FROM memberships WHERE id = $1",
            ).bind(mid).fetch_one(&mut *conn).await?,
            "ledger": json_of(&mut conn, "SELECT json_agg(l ORDER BY l.created_at) FROM ledger_entries l WHERE l.member_id = $1", mid).await?,
            "tasks_done": json_of(&mut conn,
                "SELECT json_agg(json_build_object('title', title, 'completed_at', completed_at, 'points', points_awarded, 'status', status)
                                 ORDER BY completed_at)
                 FROM tasks WHERE completed_by = $1", mid).await?,
            "tasks_assigned": json_of(&mut conn,
                "SELECT json_agg(json_build_object('title', title, 'status', status, 'due_at', due_at))
                 FROM tasks WHERE assignee_id = $1 AND status NOT IN ('completed', 'cancelled')", mid).await?,
            "transfers": json_of(&mut conn, "SELECT json_agg(t ORDER BY t.created_at) FROM transfers t WHERE t.from_id = $1 OR t.to_id = $1", mid).await?,
            "redemptions": json_of(&mut conn,
                "SELECT json_agg(json_build_object('reward', r.title, 'cost', x.cost, 'status', x.status, 'created_at', x.created_at))
                 FROM reward_redemptions x JOIN rewards r ON r.id = x.reward_id WHERE x.member_id = $1", mid).await?,
            "inbox": json_of(&mut conn, "SELECT json_agg(n ORDER BY n.created_at) FROM notifications n WHERE n.member_id = $1", mid).await?,
            "notification_prefs": json_of(&mut conn, "SELECT json_agg(p) FROM notification_prefs p WHERE p.member_id = $1", mid).await?,
        }));
    }
    let devices = json_of(&mut conn, "SELECT json_agg(json_build_object('platform', platform, 'push_provider', push_provider, 'last_seen_at', last_seen_at)) FROM devices WHERE user_id = $1", user.user_id).await?;
    let sessions = json_of(&mut conn, "SELECT json_agg(json_build_object('device_name', device_name, 'created_at', created_at, 'last_used_at', last_used_at)) FROM sessions WHERE user_id = $1 AND revoked_at IS NULL", user.user_id).await?;
    download(
        serde_json::json!({ "format": "fairteilt-export-1", "exported_at": Utc::now(), "account": account, "households": households, "devices": devices, "sessions": sessions }),
        "fairteilt-meine-daten",
    )
}

/// The whole household for admins (backup, moving away): members without secrets, rooms,
/// templates, tasks, ledger, periods, rewards, views.
pub async fn export_household(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Response> {
    let (_, role) = membership(&state, user.user_id, household_id).await?;
    if role != "admin" {
        return Err(AppError::Forbidden("Den ganzen Haushalt exportieren Admins.".into()));
    }
    let mut conn = state.db.acquire().await?;
    let h = household_id;
    let mut out = serde_json::Map::new();
    out.insert("format".into(), "fairteilt-household-1".into());
    out.insert("exported_at".into(), serde_json::json!(Utc::now()));
    out.insert("household".into(), sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT json_build_object('id', id, 'name', name, 'timezone', timezone, 'point_policy', point_policy, 'period_policy', period_policy,
                                  'goal_policy', goal_policy, 'created_at', created_at) FROM households WHERE id = $1",
    ).bind(h).fetch_one(&mut *conn).await?);
    let tables: [(&str, &'static str); 15] = [
        ("members", "SELECT json_agg(json_build_object('id', m.id, 'name', u.display_name, 'role', m.role, 'color', m.color, 'animal', m.animal,
                         'weight', m.weight, 'active', m.active, 'joined_at', m.joined_at) ORDER BY m.seq)
                     FROM memberships m JOIN users u ON u.id = m.user_id WHERE m.household_id = $1"),
        ("rooms", "SELECT json_agg(r ORDER BY r.position) FROM rooms r WHERE r.household_id = $1"),
        ("templates", "SELECT json_agg(t ORDER BY t.created_at) FROM task_templates t WHERE t.household_id = $1"),
        ("template_steps", "SELECT json_agg(s ORDER BY s.template_id, s.position) FROM template_steps s JOIN task_templates t ON t.id = s.template_id WHERE t.household_id = $1"),
        ("tasks", "SELECT json_agg(t ORDER BY t.created_at) FROM tasks t WHERE t.household_id = $1"),
        ("task_steps", "SELECT json_agg(s ORDER BY s.task_id, s.position) FROM task_steps s JOIN tasks t ON t.id = s.task_id WHERE t.household_id = $1"),
        ("task_events", "SELECT json_agg(e ORDER BY e.created_at) FROM task_events e JOIN tasks t ON t.id = e.task_id WHERE t.household_id = $1"),
        ("ledger", "SELECT json_agg(l ORDER BY l.created_at) FROM ledger_entries l WHERE l.household_id = $1"),
        ("transfers", "SELECT json_agg(t ORDER BY t.created_at) FROM transfers t WHERE t.household_id = $1"),
        ("bounties", "SELECT json_agg(b ORDER BY b.created_at) FROM bounties b JOIN tasks t ON t.id = b.task_id WHERE t.household_id = $1"),
        ("periods", "SELECT json_agg(json_build_object('period', p, 'goal', g) ORDER BY p.starts_at) FROM periods p LEFT JOIN family_goals g ON g.period_id = p.id WHERE p.household_id = $1"),
        ("settlements", "SELECT json_agg(json_build_object('settlement', s, 'lines', (SELECT json_agg(l) FROM settlement_lines l WHERE l.settlement_id = s.id)))
                         FROM settlements s JOIN periods p ON p.id = s.period_id WHERE p.household_id = $1"),
        ("rewards", "SELECT json_agg(r ORDER BY r.created_at) FROM rewards r WHERE r.household_id = $1"),
        ("redemptions", "SELECT json_agg(x ORDER BY x.created_at) FROM reward_redemptions x JOIN rewards r ON r.id = x.reward_id WHERE r.household_id = $1"),
        ("views", "SELECT json_agg(json_build_object('name', v.name, 'shared', v.shared, 'filter', v.filter, 'layout', v.layout, 'sort', v.sort))
                   FROM saved_views v WHERE v.household_id = $1 AND v.shared"),
    ];
    for (name, sql) in tables {
        out.insert(name.into(), json_of(&mut conn, sql, h).await?);
    }
    download(serde_json::Value::Object(out), "fairteilt-haushalt")
}

// ---------------------------------------------------------------- deleting

/// Removes everything personal of a membership (not the row: history points to it).
async fn forget_membership(tx: &mut Transaction<'_, Postgres>, member: Uuid) -> AppResult<()> {
    sqlx::query("UPDATE memberships SET active = false, pin_hash = NULL, pin_failed_count = 0, pin_locked_until = NULL WHERE id = $1")
        .bind(member)
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM notifications WHERE member_id = $1").bind(member).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM notification_prefs WHERE member_id = $1").bind(member).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM saved_views WHERE owner_id = $1").bind(member).execute(&mut **tx).await?;
    // Their open tasks become open again.
    sqlx::query("UPDATE tasks SET assignee_id = NULL, status = 'open', started_at = NULL, remind_at = NULL WHERE assignee_id = $1 AND status IN ('claimed', 'in_progress')")
        .bind(member)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

/// Removes everything personal of a user account and keeps an anonymous row.
async fn forget_user(tx: &mut Transaction<'_, Postgres>, user: Uuid) -> AppResult<()> {
    sqlx::query(
        "UPDATE users SET email = NULL, password_hash = NULL, display_name = 'Gelöschtes Konto', deleted_at = now(), updated_at = now() WHERE id = $1",
    )
    .bind(user)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM sessions WHERE user_id = $1").bind(user).execute(&mut **tx).await?;
    sqlx::query("DELETE FROM devices WHERE user_id = $1").bind(user).execute(&mut **tx).await?;
    Ok(())
}

/// Deletes a household with everything in it; child profiles that belonged only to it go too.
async fn delete_household_tx(tx: &mut Transaction<'_, Postgres>, household: Uuid) -> AppResult<()> {
    let users: Vec<Uuid> = sqlx::query_scalar("SELECT user_id FROM memberships WHERE household_id = $1").bind(household).fetch_all(&mut **tx).await?;
    // Rows that point to members without a cascade go first (in this order), so that the
    // reference checks never see a member removed before its history.
    for sql in [
        "DELETE FROM periods WHERE household_id = $1",
        "DELETE FROM tasks WHERE household_id = $1",
        "DELETE FROM task_templates WHERE household_id = $1",
        "DELETE FROM ledger_entries WHERE household_id = $1",
        "DELETE FROM transfers WHERE household_id = $1",
        "DELETE FROM reward_redemptions WHERE reward_id IN (SELECT id FROM rewards WHERE household_id = $1)",
        "DELETE FROM rewards WHERE household_id = $1",
        "DELETE FROM invites WHERE household_id = $1",
        "DELETE FROM households WHERE id = $1",
    ] {
        sqlx::query(sql).bind(household).execute(&mut **tx).await?;
    }
    // Profiles without login of their own and without any other household.
    sqlx::query(
        "DELETE FROM users u WHERE u.id = ANY($1) AND u.email IS NULL AND NOT EXISTS (SELECT 1 FROM memberships m WHERE m.user_id = u.id)",
    )
    .bind(&users)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct DeleteAccountRequest {
    pub password: String,
}

/// Delete your own account (password required). Child profiles are removed by an admin.
pub async fn delete_me(State(state): State<AppState>, user: AuthUser, Json(req): Json<DeleteAccountRequest>) -> AppResult<StatusCode> {
    let hash: Option<String> = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1").bind(user.user_id).fetch_one(&state.db).await?;
    let Some(hash) = hash else {
        return Err(AppError::Forbidden("Kinderprofile löscht ein Admin unter Familie.".into()));
    };
    if !verify_password(&req.password, &hash) {
        return Err(AppError::BadRequest("Das Passwort stimmt nicht.".into()));
    }
    let mut tx = state.db.begin().await?;
    #[derive(sqlx::FromRow)]
    struct Mine {
        id: Uuid,
        household_id: Uuid,
        name: String,
        role: String,
        others: i64,
        other_admins: i64,
    }
    let mine = sqlx::query_as::<_, Mine>(
        "SELECT m.id, m.household_id, h.name, m.role,
                (SELECT COUNT(*) FROM memberships o WHERE o.household_id = m.household_id AND o.active AND o.id <> m.id) AS others,
                (SELECT COUNT(*) FROM memberships o WHERE o.household_id = m.household_id AND o.active AND o.id <> m.id AND o.role = 'admin') AS other_admins
         FROM memberships m JOIN households h ON h.id = m.household_id WHERE m.user_id = $1 AND m.active",
    )
    .bind(user.user_id)
    .fetch_all(&mut *tx)
    .await?;
    if let Some(m) = mine.iter().find(|m| m.role == "admin" && m.others > 0 && m.other_admins == 0) {
        return Err(AppError::Conflict(format!(
            "In „{}“ bist du der einzige Admin. Bestimme zuerst einen anderen Admin oder lösche den Haushalt.",
            m.name
        )));
    }
    let mut touched = Vec::new();
    for m in &mine {
        if m.others == 0 {
            delete_household_tx(&mut tx, m.household_id).await?;
        } else {
            forget_membership(&mut tx, m.id).await?;
            touched.push(m.household_id);
        }
    }
    forget_user(&mut tx, user.user_id).await?;
    tx.commit().await?;
    for hid in touched {
        state.live.notify(hid, r#"{"type":"tasks_changed"}"#);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct DeleteHouseholdRequest {
    /// The household's name, typed again as a safety check.
    pub confirm_name: String,
}

/// Admins delete the whole household with all tasks, points and history.
pub async fn delete_household(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<DeleteHouseholdRequest>,
) -> AppResult<StatusCode> {
    let (_, role) = membership(&state, user.user_id, household_id).await?;
    if role != "admin" {
        return Err(AppError::Forbidden("Den Haushalt löschen Admins.".into()));
    }
    let name: String = sqlx::query_scalar("SELECT name FROM households WHERE id = $1").bind(household_id).fetch_one(&state.db).await?;
    if req.confirm_name.trim() != name.trim() {
        return Err(AppError::BadRequest("Bitte den Namen des Haushalts genau so eintippen.".into()));
    }
    let mut tx = state.db.begin().await?;
    delete_household_tx(&mut tx, household_id).await?;
    tx.commit().await?;
    state.live.notify(household_id, r#"{"type":"household_deleted"}"#);
    Ok(StatusCode::NO_CONTENT)
}

/// Admins remove a member. A child profile without its own login is anonymized as well.
/// The last admin cannot be removed; nobody removes themselves here (that is "Konto löschen").
pub async fn remove_member(State(state): State<AppState>, user: AuthUser, Path((household_id, member_id)): Path<(Uuid, Uuid)>) -> AppResult<StatusCode> {
    let (me, role) = membership(&state, user.user_id, household_id).await?;
    if role != "admin" {
        return Err(AppError::Forbidden("Mitglieder entfernen Admins.".into()));
    }
    if member_id == me {
        return Err(AppError::BadRequest("Dich selbst entfernst du über „Konto löschen“.".into()));
    }
    let mut tx = state.db.begin().await?;
    let target: Option<(Uuid, String, Option<String>)> = sqlx::query_as(
        "SELECT m.user_id, m.role, u.email FROM memberships m JOIN users u ON u.id = m.user_id WHERE m.id = $1 AND m.household_id = $2 AND m.active FOR UPDATE OF m",
    )
    .bind(member_id)
    .bind(household_id)
    .fetch_optional(&mut *tx)
    .await?;
    let (target_user, _, email) = target.ok_or(AppError::NotFound)?;
    forget_membership(&mut tx, member_id).await?;
    let elsewhere: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM memberships WHERE user_id = $1 AND active)").bind(target_user).fetch_one(&mut *tx).await?;
    if email.is_none() && !elsewhere {
        forget_user(&mut tx, target_user).await?;
    } else {
        // Their sessions in this household (PIN) end; their own account stays.
        sqlx::query("DELETE FROM sessions WHERE user_id = $1 AND device_name IN ('Familien-Tablet', 'Wandanzeige')").bind(target_user).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    state.live.notify(household_id, r#"{"type":"tasks_changed"}"#);
    Ok(StatusCode::NO_CONTENT)
}
