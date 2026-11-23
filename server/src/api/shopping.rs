//! Shopping lists (M8, KONZEPT §10).
//!
//! Every list belongs to an "auf Abruf" template ("Einkaufen"). The first item on an empty
//! list triggers a round; adding items never earns XP. Checking an item while someone is
//! shopping puts it in the cart; "Einkauf fertig" (the task's `done`) moves the cart into the
//! history and credits the shopper. Checking an item while nobody is shopping means it was
//! bought on the side: it leaves the list right away, and an emptied list closes its open
//! round without XP.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use chrono::{DateTime, Utc};
use punkte_core::{Assignment, Schedule, Section, StepOrder, clean_name, clean_note, item_key, merge_note};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::api::households::membership;
use crate::api::rooms::planner;
use crate::api::tasks::{TaskRow, default_range, household, lock_task, record_event, refund_bounties, trigger};
use crate::auth::AuthUser;
use crate::error::{AppError, AppResult};
use crate::inbox::{self, Category, Note};
use crate::AppState;

const MAX_LISTS: i64 = 10;
const MAX_ITEMS: i64 = 200;
const SUGGESTIONS: i64 = 40;
/// Default "fällig nach" for a new shopping task.
const DUE_AFTER_DAYS: u32 = 2;

// ---------------------------------------------------------------- DTOs

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct ShoppingItemDto {
    pub id: Uuid,
    pub list_id: Uuid,
    pub name: String,
    pub note: Option<String>,
    pub section: Section,
    pub urgent: bool,
    pub added_by: Option<Uuid>,
    pub added_at: DateTime<Utc>,
    /// In the cart (someone is shopping right now).
    pub checked_by: Option<Uuid>,
    pub checked_at: Option<DateTime<Utc>>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct ShoppingListDto {
    pub id: Uuid,
    pub name: String,
    pub template_id: Option<Uuid>,
    /// The open round of the list's task ("Einkaufen"), if any.
    pub task_id: Option<Uuid>,
    pub items: Vec<ShoppingItemDto>,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct SuggestionDto {
    pub name: String,
    pub section: Section,
    pub times: i32,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct ShoppingDto {
    pub lists: Vec<ShoppingListDto>,
    /// What the family buys most, for quick adding.
    pub suggestions: Vec<SuggestionDto>,
}

#[derive(sqlx::FromRow)]
struct ItemRow {
    id: Uuid,
    list_id: Uuid,
    name: String,
    note: Option<String>,
    section: String,
    urgent: bool,
    added_by: Option<Uuid>,
    added_at: DateTime<Utc>,
    checked_by: Option<Uuid>,
    checked_at: Option<DateTime<Utc>>,
}

impl From<ItemRow> for ShoppingItemDto {
    fn from(r: ItemRow) -> Self {
        ShoppingItemDto {
            id: r.id,
            list_id: r.list_id,
            name: r.name,
            note: r.note,
            section: Section::parse(&r.section),
            urgent: r.urgent,
            added_by: r.added_by,
            added_at: r.added_at,
            checked_by: r.checked_by,
            checked_at: r.checked_at,
        }
    }
}

/// Columns of `ItemRow`, as a macro so queries stay literal (static) SQL strings.
macro_rules! item_columns {
    () => {
        "id, list_id, name, note, section, urgent, added_by, added_at, checked_by, checked_at"
    };
}

// ---------------------------------------------------------------- helpers

/// New households (and existing ones on first use) get "Supermarkt" with the task "Einkaufen".
pub(crate) async fn ensure_default(db: &PgPool, household_id: Uuid) -> AppResult<()> {
    let mut tx = db.begin().await?;
    let seeded: bool = sqlx::query_scalar("SELECT shopping_seeded FROM households WHERE id = $1 FOR UPDATE")
        .bind(household_id)
        .fetch_one(&mut *tx)
        .await?;
    if seeded {
        return Ok(());
    }
    create_list_tx(&mut tx, household_id, "Supermarkt", "Einkaufen").await?;
    sqlx::query("UPDATE households SET shopping_seeded = true WHERE id = $1").bind(household_id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

async fn create_list_tx(tx: &mut Transaction<'_, Postgres>, household_id: Uuid, name: &str, task_title: &str) -> AppResult<Uuid> {
    let policy: serde_json::Value = sqlx::query_scalar("SELECT point_policy FROM households WHERE id = $1").bind(household_id).fetch_one(&mut **tx).await?;
    let policy: punkte_core::PointPolicy = serde_json::from_value(policy).map_err(anyhow::Error::from)?;
    let range = default_range(&policy);
    let template_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO task_templates (id, household_id, title, icon, kind, points_min, points_default, points_max, schedule, assignment, step_order)
         VALUES ($1, $2, $3, 'cart', 'regular', $4, $5, $6, $7, $8, $9)",
    )
    .bind(template_id)
    .bind(household_id)
    .bind(task_title)
    .bind(range.min)
    .bind(range.default)
    .bind(range.max)
    .bind(serde_json::to_value(Schedule::OnCall { due_after_days: Some(DUE_AFTER_DAYS) }).map_err(anyhow::Error::from)?)
    .bind(serde_json::to_value(Assignment::Open).map_err(anyhow::Error::from)?)
    .bind(StepOrder::Free.as_str())
    .execute(&mut **tx)
    .await?;
    let list_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO shopping_lists (id, household_id, name, template_id, position)
         VALUES ($1, $2, $3, $4, (SELECT COALESCE(MAX(position) + 1, 0) FROM shopping_lists WHERE household_id = $2))",
    )
    .bind(list_id)
    .bind(household_id)
    .bind(name)
    .bind(template_id)
    .execute(&mut **tx)
    .await?;
    Ok(list_id)
}

/// Unchecked items on the lists of a template (they call the next round).
pub(crate) async fn has_open_items(tx: &mut Transaction<'_, Postgres>, template_id: Uuid) -> AppResult<bool> {
    Ok(sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM shopping_items i JOIN shopping_lists l ON l.id = i.list_id
                        WHERE l.template_id = $1 AND NOT l.archived AND i.checked_at IS NULL)",
    )
    .bind(template_id)
    .fetch_one(&mut **tx)
    .await?)
}

/// Moves items into the history (for suggestions) and off the list.
async fn bought(tx: &mut Transaction<'_, Postgres>, household_id: Uuid, item_ids: &[Uuid], now: DateTime<Utc>) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO shopping_history (household_id, key, name, section, times, last_bought)
         SELECT $1, key, name, section, 1, $3 FROM shopping_items WHERE id = ANY($2)
         ON CONFLICT (household_id, key) DO UPDATE
         SET name = EXCLUDED.name, section = EXCLUDED.section, times = shopping_history.times + 1, last_bought = EXCLUDED.last_bought",
    )
    .bind(household_id)
    .bind(item_ids)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    sqlx::query("DELETE FROM shopping_items WHERE id = ANY($1)").bind(item_ids).execute(&mut **tx).await?;
    Ok(())
}

/// "Einkauf fertig": the cart leaves the list. Finishing with an empty cart while items are
/// still waiting is refused (no XP for a trip without shopping).
pub(crate) async fn clear_cart(tx: &mut Transaction<'_, Postgres>, task: &TaskRow, now: DateTime<Utc>) -> AppResult<()> {
    let Some(list_id) = task.list_id else { return Ok(()) };
    let rows: Vec<(Uuid, bool)> = sqlx::query_as("SELECT id, checked_at IS NOT NULL FROM shopping_items WHERE list_id = $1 FOR UPDATE")
        .bind(list_id)
        .fetch_all(&mut **tx)
        .await?;
    let cart: Vec<Uuid> = rows.iter().filter(|(_, c)| *c).map(|(id, _)| *id).collect();
    if cart.is_empty() && !rows.is_empty() {
        return Err(AppError::Conflict("Hake erst ab, was du eingekauft hast.".into()));
    }
    bought(tx, task.household_id, &cart, now).await
}

/// The open round of the list's task, if any: (task id, status, assignee).
async fn open_round(tx: &mut Transaction<'_, Postgres>, template_id: Option<Uuid>) -> AppResult<Option<(Uuid, String, Option<Uuid>)>> {
    let Some(template_id) = template_id else { return Ok(None) };
    Ok(sqlx::query_as(
        "SELECT id, status, assignee_id FROM tasks WHERE template_id = $1 AND status NOT IN ('completed', 'cancelled') LIMIT 1",
    )
    .bind(template_id)
    .fetch_optional(&mut **tx)
    .await?)
}

/// Nothing left to buy and nobody shopping: the round is closed quietly ("nicht mehr nötig").
async fn close_if_empty(tx: &mut Transaction<'_, Postgres>, list: &ListRow, actor: Uuid) -> AppResult<bool> {
    let left: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM shopping_items WHERE list_id = $1)").bind(list.id).fetch_one(&mut **tx).await?;
    if left {
        return Ok(false);
    }
    let Some((task_id, status, _)) = open_round(tx, list.template_id).await? else { return Ok(false) };
    if status != "open" && status != "claimed" {
        return Ok(false);
    }
    let task = lock_task(tx, task_id).await?;
    sqlx::query("UPDATE tasks SET status = 'cancelled', started_at = NULL, remind_at = NULL, updated_at = now() WHERE id = $1")
        .bind(task_id)
        .execute(&mut **tx)
        .await?;
    refund_bounties(tx, &task, actor).await?;
    record_event(tx, task_id, actor, "not_needed", serde_json::json!({})).await?;
    Ok(true)
}

#[derive(sqlx::FromRow)]
struct ListRow {
    id: Uuid,
    household_id: Uuid,
    name: String,
    template_id: Option<Uuid>,
}

async fn list_of(state: &AppState, user: &AuthUser, list_id: Uuid) -> AppResult<(ListRow, Uuid, String)> {
    let list = sqlx::query_as::<_, ListRow>("SELECT id, household_id, name, template_id FROM shopping_lists WHERE id = $1 AND NOT archived")
        .bind(list_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let (mid, role) = membership(state, user.user_id, list.household_id).await?;
    Ok((list, mid, role))
}

async fn item_of(state: &AppState, user: &AuthUser, item_id: Uuid) -> AppResult<(ItemRow, ListRow, Uuid, String)> {
    let item = sqlx::query_as::<_, ItemRow>(concat!("SELECT ", item_columns!(), " FROM shopping_items WHERE id = $1"))
        .bind(item_id)
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::NotFound)?;
    let (list, mid, role) = list_of(state, user, item.list_id).await?;
    Ok((item, list, mid, role))
}

fn changed(state: &AppState, household: Uuid, tasks: bool) {
    state.live.notify(household, r#"{"type":"list_changed"}"#);
    if tasks {
        state.live.notify(household, r#"{"type":"tasks_changed"}"#);
    }
}

// ---------------------------------------------------------------- lists

pub async fn overview(State(state): State<AppState>, user: AuthUser, Path(household_id): Path<Uuid>) -> AppResult<Json<ShoppingDto>> {
    membership(&state, user.user_id, household_id).await?;
    ensure_default(&state.db, household_id).await?;
    let lists = sqlx::query_as::<_, ListRow>(
        "SELECT id, household_id, name, template_id FROM shopping_lists WHERE household_id = $1 AND NOT archived ORDER BY position, created_at",
    )
    .bind(household_id)
    .fetch_all(&state.db)
    .await?;
    let ids: Vec<Uuid> = lists.iter().map(|l| l.id).collect();
    let mut items: Vec<ItemRow> =
        sqlx::query_as::<_, ItemRow>(concat!("SELECT ", item_columns!(), " FROM shopping_items WHERE list_id = ANY($1) ORDER BY added_at"))
            .bind(&ids)
            .fetch_all(&state.db)
            .await?;
    let rounds: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT template_id, id FROM tasks WHERE household_id = $1 AND template_id IS NOT NULL AND status NOT IN ('completed', 'cancelled')",
    )
    .bind(household_id)
    .fetch_all(&state.db)
    .await?;
    let lists = lists
        .into_iter()
        .map(|l| ShoppingListDto {
            id: l.id,
            task_id: l.template_id.and_then(|t| rounds.iter().find(|(tid, _)| *tid == t).map(|(_, id)| *id)),
            items: items.extract_if(.., |i| i.list_id == l.id).map(ShoppingItemDto::from).collect(),
            name: l.name,
            template_id: l.template_id,
        })
        .collect();
    let suggestions: Vec<(String, String, i32)> = sqlx::query_as(
        "SELECT name, section, times FROM shopping_history WHERE household_id = $1 ORDER BY times DESC, last_bought DESC LIMIT $2",
    )
    .bind(household_id)
    .bind(SUGGESTIONS)
    .fetch_all(&state.db)
    .await?;
    let suggestions = suggestions.into_iter().map(|(name, section, times)| SuggestionDto { name, section: Section::parse(&section), times }).collect();
    Ok(Json(ShoppingDto { lists, suggestions }))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct ListRequest {
    pub name: String,
    /// Title of the list's task; default "<Name> einkaufen".
    pub task_title: Option<String>,
}

fn check_list_name(name: &str) -> AppResult<String> {
    let n = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if n.is_empty() || n.chars().count() > 40 {
        return Err(AppError::BadRequest("Der Name der Liste muss 1 bis 40 Zeichen lang sein.".into()));
    }
    Ok(n)
}

/// A new list with its own "auf Abruf" task (adults).
pub async fn create_list(
    State(state): State<AppState>,
    user: AuthUser,
    Path(household_id): Path<Uuid>,
    Json(req): Json<ListRequest>,
) -> AppResult<(StatusCode, Json<Uuid>)> {
    planner(&state, &user, household_id).await?;
    ensure_default(&state.db, household_id).await?;
    let name = check_list_name(&req.name)?;
    let title = req.task_title.as_deref().map(str::trim).filter(|t| !t.is_empty()).map_or_else(|| format!("{name} einkaufen"), str::to_string);
    if title.chars().count() > 80 {
        return Err(AppError::BadRequest("Der Titel muss 1 bis 80 Zeichen lang sein.".into()));
    }
    let mut tx = state.db.begin().await?;
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shopping_lists WHERE household_id = $1 AND NOT archived")
        .bind(household_id)
        .fetch_one(&mut *tx)
        .await?;
    if count >= MAX_LISTS {
        return Err(AppError::BadRequest(format!("Höchstens {MAX_LISTS} Listen.")));
    }
    let id = create_list_tx(&mut tx, household_id, &name, &title).await?;
    tx.commit().await?;
    changed(&state, household_id, false);
    Ok((StatusCode::CREATED, Json(id)))
}

pub async fn rename_list(State(state): State<AppState>, user: AuthUser, Path(list_id): Path<Uuid>, Json(req): Json<ListRequest>) -> AppResult<StatusCode> {
    let (list, _, _) = list_of(&state, &user, list_id).await?;
    planner(&state, &user, list.household_id).await?;
    let name = check_list_name(&req.name)?;
    sqlx::query("UPDATE shopping_lists SET name = $2 WHERE id = $1").bind(list_id).bind(&name).execute(&state.db).await?;
    changed(&state, list.household_id, false);
    Ok(StatusCode::NO_CONTENT)
}

/// Removes a list together with its task (an open round is skipped, bonuses go back).
pub async fn delete_list(State(state): State<AppState>, user: AuthUser, Path(list_id): Path<Uuid>) -> AppResult<StatusCode> {
    let (list, me, _) = list_of(&state, &user, list_id).await?;
    planner(&state, &user, list.household_id).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE shopping_lists SET archived = true WHERE id = $1").bind(list_id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM shopping_items WHERE list_id = $1").bind(list_id).execute(&mut *tx).await?;
    if let Some(template_id) = list.template_id {
        if let Some((task_id, _, _)) = open_round(&mut tx, Some(template_id)).await? {
            let task = lock_task(&mut tx, task_id).await?;
            sqlx::query("UPDATE tasks SET status = 'cancelled', started_at = NULL, remind_at = NULL, updated_at = now() WHERE id = $1")
                .bind(task_id)
                .execute(&mut *tx)
                .await?;
            refund_bounties(&mut tx, &task, me).await?;
        }
        sqlx::query("UPDATE task_templates SET archived = true WHERE id = $1").bind(template_id).execute(&mut *tx).await?;
    }
    tx.commit().await?;
    changed(&state, list.household_id, true);
    state.live.notify(list.household_id, r#"{"type":"points_changed"}"#);
    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------- items

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct AddItemRequest {
    /// Chosen by the app, so a change made offline can refer to the item before it exists.
    pub id: Option<Uuid>,
    pub name: String,
    pub note: Option<String>,
    /// Default: where the family bought it last time, else "Sonstiges".
    pub section: Option<Section>,
    #[serde(default)]
    pub urgent: bool,
    /// Wall display: who wrote it down (a member of the household; adding books nothing).
    pub as_member: Option<Uuid>,
}

/// Anyone writes something down. No XP. The same item twice becomes one line.
pub async fn add_item(
    State(state): State<AppState>,
    user: AuthUser,
    Path(list_id): Path<Uuid>,
    Json(req): Json<AddItemRequest>,
) -> AppResult<(StatusCode, Json<ShoppingItemDto>)> {
    let (list, me, _) = list_of(&state, &user, list_id).await?;
    let name = clean_name(&req.name).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let note = clean_note(req.note.as_deref()).map_err(|e| AppError::BadRequest(e.to_string()))?;
    let key = item_key(&name);
    if key.is_empty() {
        return Err(AppError::BadRequest("Der Artikel braucht einen Namen.".into()));
    }
    let hh = household(&state.db, list.household_id).await?;
    let mut tx = state.db.begin().await?;
    let author = match req.as_member {
        Some(m) if m != me => {
            let ok: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM memberships WHERE id = $1 AND household_id = $2 AND active)")
                .bind(m)
                .bind(list.household_id)
                .fetch_one(&mut *tx)
                .await?;
            if !ok {
                return Err(AppError::BadRequest("Diese Person ist nicht im Haushalt.".into()));
            }
            m
        }
        _ => me,
    };
    sqlx::query("SELECT id FROM shopping_lists WHERE id = $1 FOR UPDATE").bind(list_id).execute(&mut *tx).await?;
    let existing = sqlx::query_as::<_, ItemRow>(concat!("SELECT ", item_columns!(), " FROM shopping_items WHERE list_id = $1 AND key = $2 AND checked_at IS NULL LIMIT 1"))
        .bind(list_id)
        .bind(&key)
        .fetch_optional(&mut *tx)
        .await?;
    let (item_id, created) = if let Some(e) = existing {
        let merged = merge_note(e.note.as_deref(), note.as_deref());
        sqlx::query("UPDATE shopping_items SET note = $2, urgent = urgent OR $3, section = COALESCE($4, section) WHERE id = $1")
            .bind(e.id)
            .bind(merged)
            .bind(req.urgent)
            .bind(req.section.map(Section::as_str))
            .execute(&mut *tx)
            .await?;
        (e.id, false)
    } else {
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM shopping_items WHERE list_id = $1").bind(list_id).fetch_one(&mut *tx).await?;
        if count >= MAX_ITEMS {
            return Err(AppError::BadRequest(format!("Höchstens {MAX_ITEMS} Artikel auf einer Liste.")));
        }
        let section = match req.section {
            Some(s) => s.as_str().to_string(),
            None => sqlx::query_scalar("SELECT section FROM shopping_history WHERE household_id = $1 AND key = $2")
                .bind(list.household_id)
                .bind(&key)
                .fetch_optional(&mut *tx)
                .await?
                .unwrap_or_else(|| Section::Other.as_str().to_string()),
        };
        let id = req.id.unwrap_or_else(Uuid::new_v4);
        let taken: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM shopping_items WHERE id = $1)").bind(id).fetch_one(&mut *tx).await?;
        if taken {
            return Err(AppError::Conflict("Diesen Artikel gibt es schon.".into()));
        }
        sqlx::query(
            "INSERT INTO shopping_items (id, list_id, name, key, note, section, urgent, added_by) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(id)
        .bind(list_id)
        .bind(&name)
        .bind(&key)
        .bind(&note)
        .bind(&section)
        .bind(req.urgent)
        .bind(author)
        .execute(&mut *tx)
        .await?;
        (id, true)
    };

    // The list calls its task; whoever is shopping right now hears about it.
    let mut tasks_changed = false;
    if let Some(template_id) = list.template_id {
        tasks_changed = trigger(&mut tx, template_id, hh.tz, Some(author), req.urgent).await?.is_some() || req.urgent;
        if created && let Some((task_id, status, Some(shopper))) = open_round(&mut tx, Some(template_id)).await?
            && status == "in_progress"
            && shopper != author
        {
            let who = inbox::name(&mut tx, author).await?;
            inbox::add(
                &mut tx,
                Note::new(shopper, "list_item", Category::Tasks, format!("{who}: {name} noch mit?"))
                    .body(format!("Steht jetzt auf der Liste „{}“.", list.name))
                    .about("task", task_id),
            )
            .await?;
        }
    }
    let item = sqlx::query_as::<_, ItemRow>(concat!("SELECT ", item_columns!(), " FROM shopping_items WHERE id = $1")).bind(item_id).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    changed(&state, list.household_id, tasks_changed);
    Ok((if created { StatusCode::CREATED } else { StatusCode::OK }, Json(item.into())))
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct UpdateItemRequest {
    pub note: Option<String>,
    pub section: Option<Section>,
    pub urgent: Option<bool>,
}

fn may_change(item: &ItemRow, me: Uuid, role: &str) -> AppResult<()> {
    if role == "child" && item.added_by != Some(me) {
        return Err(AppError::Forbidden("Das hat jemand anderes aufgeschrieben.".into()));
    }
    Ok(())
}

pub async fn update_item(
    State(state): State<AppState>,
    user: AuthUser,
    Path(item_id): Path<Uuid>,
    Json(req): Json<UpdateItemRequest>,
) -> AppResult<Json<ShoppingItemDto>> {
    let (item, list, me, role) = item_of(&state, &user, item_id).await?;
    may_change(&item, me, &role)?;
    let note = match &req.note {
        Some(n) => clean_note(Some(n)).map_err(|e| AppError::BadRequest(e.to_string()))?,
        None => item.note.clone(),
    };
    let section = req.section.map_or(item.section.clone(), |s| s.as_str().to_string());
    let urgent = req.urgent.unwrap_or(item.urgent);
    let hh = household(&state.db, list.household_id).await?;
    let mut tx = state.db.begin().await?;
    sqlx::query("UPDATE shopping_items SET note = $2, section = $3, urgent = $4 WHERE id = $1")
        .bind(item_id)
        .bind(&note)
        .bind(&section)
        .bind(urgent)
        .execute(&mut *tx)
        .await?;
    if urgent && !item.urgent && let Some(template_id) = list.template_id {
        trigger(&mut tx, template_id, hh.tz, Some(me), true).await?;
    }
    let row = sqlx::query_as::<_, ItemRow>(concat!("SELECT ", item_columns!(), " FROM shopping_items WHERE id = $1")).bind(item_id).fetch_one(&mut *tx).await?;
    tx.commit().await?;
    changed(&state, list.household_id, urgent != item.urgent);
    Ok(Json(row.into()))
}

pub async fn delete_item(State(state): State<AppState>, user: AuthUser, Path(item_id): Path<Uuid>) -> AppResult<StatusCode> {
    let (item, list, me, role) = item_of(&state, &user, item_id).await?;
    may_change(&item, me, &role)?;
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM shopping_items WHERE id = $1").bind(item_id).execute(&mut *tx).await?;
    let closed = close_if_empty(&mut tx, &list, me).await?;
    tx.commit().await?;
    changed(&state, list.household_id, closed);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Deserialize)]
pub struct CheckItemRequest {
    pub checked: bool,
}

#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[derive(Serialize)]
pub struct CheckItemResponse {
    /// The item, still on the list (in the cart or back out of it); none when it left the list.
    pub item: Option<ShoppingItemDto>,
    /// Checking started the shopping round for the caller.
    pub started_task: Option<Uuid>,
}

/// Checking an item: in the cart while the caller is (or now starts) shopping; otherwise it
/// was bought on the side and leaves the list.
pub async fn check_item(
    State(state): State<AppState>,
    user: AuthUser,
    Path(item_id): Path<Uuid>,
    Json(req): Json<CheckItemRequest>,
) -> AppResult<Json<CheckItemResponse>> {
    let (_, list, me, _) = item_of(&state, &user, item_id).await?;
    let now = Utc::now();
    let mut tx = state.db.begin().await?;
    let item = sqlx::query_as::<_, ItemRow>(concat!("SELECT ", item_columns!(), " FROM shopping_items WHERE id = $1 FOR UPDATE"))
        .bind(item_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    let round = open_round(&mut tx, list.template_id).await?;
    let mut started_task = None;
    let mut tasks_changed = false;
    let mut left = false;

    if !req.checked {
        sqlx::query("UPDATE shopping_items SET checked_by = NULL, checked_at = NULL WHERE id = $1").bind(item_id).execute(&mut *tx).await?;
    } else if item.checked_at.is_some() {
        // Already in the cart (a repeated request): nothing to do.
    } else {
        // The caller shops: the round is (or becomes) theirs, like checking a step starts a task.
        // "In the shop" since the first item in the cart (no timer; `started_at` only marks
        // which items were written down later). Helpers on the round shop too.
        let helping = match &round {
            Some((task_id, _, _)) => sqlx::query_scalar::<_, bool>("SELECT EXISTS (SELECT 1 FROM task_helpers WHERE task_id = $1 AND member_id = $2)")
                .bind(*task_id)
                .bind(me)
                .fetch_one(&mut *tx)
                .await?,
            None => false,
        };
        let shopping = match &round {
            Some((task_id, status, _)) if status == "in_progress" => Some(*task_id),
            Some((task_id, status, assignee))
                if (status == "open" || status == "claimed") && (assignee.is_none() || *assignee == Some(me) || helping) =>
            {
                lock_task(&mut tx, *task_id).await?;
                sqlx::query(
                    "UPDATE tasks SET status = 'in_progress', assignee_id = COALESCE(assignee_id, $2), started_at = $3, remind_at = NULL, updated_at = $3 WHERE id = $1",
                )
                .bind(*task_id)
                .bind(me)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                record_event(&mut tx, *task_id, me, "started", serde_json::json!({})).await?;
                started_task = Some(*task_id);
                tasks_changed = true;
                Some(*task_id)
            }
            _ => None,
        };
        match shopping {
            // Someone shops (the caller or, with a running round, anyone helping in the shop).
            Some(_) => {
                sqlx::query("UPDATE shopping_items SET checked_by = $2, checked_at = $3 WHERE id = $1")
                    .bind(item_id)
                    .bind(me)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
            }
            // Bought on the side: off the list, no XP.
            None => {
                bought(&mut tx, list.household_id, &[item_id], now).await?;
                left = true;
                tasks_changed |= close_if_empty(&mut tx, &list, me).await?;
            }
        }
    }
    let item = if left {
        None
    } else {
        Some(sqlx::query_as::<_, ItemRow>(concat!("SELECT ", item_columns!(), " FROM shopping_items WHERE id = $1")).bind(item_id).fetch_one(&mut *tx).await?.into())
    };
    tx.commit().await?;
    changed(&state, list.household_id, tasks_changed);
    Ok(Json(CheckItemResponse { item, started_task }))
}

