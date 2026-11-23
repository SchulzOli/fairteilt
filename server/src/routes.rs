//! HTTP routes. Everything under `/api/v1`.

use axum::extract::State;
use axum::http::{HeaderValue, Method, header};
use axum::routing::{get, patch, post, put};
use axum::{Json, Router};
use punkte_core::{MemberPeriodInput, PeriodPolicy, PointPolicy, Settlement};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::error::{AppError, AppResult};
use crate::{AppState, api};

pub fn router(state: AppState, cors_origins: &[String]) -> Router {
    let origins: Vec<HeaderValue> = cors_origins.iter().filter_map(|o| o.parse().ok()).collect();
    let cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::HeaderName::from_static("idempotency-key")])
        .expose_headers([header::CONTENT_DISPOSITION]);

    let v1 = Router::new()
        .route("/meta", get(meta))
        .route("/preview/settlement", post(preview_settlement))
        .route("/auth/register", post(api::auth::register))
        .route("/auth/login", post(api::auth::login))
        .route("/auth/google", post(api::auth::google))
        .route("/auth/refresh", post(api::auth::refresh))
        .route("/auth/logout", post(api::auth::logout))
        .route("/me", get(api::auth::me).delete(api::account::delete_me))
        .route("/me/export", get(api::account::export_me))
        .route("/households/{id}/export", get(api::account::export_household))
        .route("/households/{id}/delete", post(api::account::delete_household))
        .route("/households", post(api::households::create))
        .route("/households/{id}/invites", post(api::households::create_invite))
        .route("/invites/accept", post(api::households::accept_invite))
        .route("/invites/join", post(api::members::join_without_account))
        .route("/auth/pin", post(api::members::pin_login))
        .route("/households/{id}/members", get(api::members::list))
        .route("/households/{id}/members/{mid}", patch(api::members::update).delete(api::account::remove_member))
        .route("/households/{id}/members/{mid}/pin", put(api::members::set_pin))
        .route("/households/{id}/children", post(api::members::create_child))
        .route("/households/{id}/rooms", get(api::rooms::list).post(api::rooms::create))
        .route("/households/{id}/rooms/{rid}", patch(api::rooms::update).delete(api::rooms::delete))
        .route("/households/{id}/templates", get(api::tasks::list_templates).post(api::tasks::create_template))
        .route("/households/{id}/tasks", get(api::tasks::list_tasks))
        .route("/households/{id}/live", get(crate::live::live))
        .route("/tasks/{tid}", get(api::tasks::get_task))
        .route("/tasks/{tid}/action", post(api::tasks::action))
        .route("/tasks/{tid}/room", put(api::templates::move_room))
        .route("/templates/{tid}", patch(api::templates::update_template).delete(api::templates::delete_template))
        .route("/tasks/{tid}/steps/{sid}/check", post(api::tasks::check_step))
        .route("/tasks/{tid}/review", post(api::tasks::review))
        .route("/tasks/{tid}/bounty", post(api::tasks::pledge))
        .route("/households/{id}/accounts", get(api::points::accounts))
        .route("/households/{id}/ledger", get(api::points::history))
        .route("/households/{id}/ledger/{eid}/reverse", post(api::points::reverse))
        .route("/households/{id}/period", get(api::period::current))
        .route("/households/{id}/settlements", get(api::period::list))
        .route("/settlements/{sid}/approve", post(api::period::approve))
        .route("/households/{id}/rules", get(api::period::rules).put(api::period::update_rules))
        .route("/households/{id}/goal", put(api::period::update_goal))
        .route("/households/{id}/inbox", get(api::inbox::list))
        .route("/households/{id}/inbox/unread", get(api::inbox::unread))
        .route("/households/{id}/inbox/read", post(api::inbox::mark_read))
        .route("/households/{id}/notification-prefs", get(api::inbox::prefs).put(api::inbox::update_prefs))
        .route("/devices", post(api::inbox::register_device).delete(api::inbox::unregister_device))
        .route("/tasks/{tid}/schedule", put(api::plan::schedule))
        .route("/tasks/{tid}/assign", put(api::plan::assign))
        .route("/households/{id}/calendar", get(api::plan::calendar))
        .route("/households/{id}/board", get(api::plan::board).put(api::plan::move_card))
        .route("/households/{id}/views", get(api::plan::views).post(api::plan::create_view))
        .route("/views/{vid}", put(api::plan::update_view).delete(api::plan::delete_view))
        .route("/templates/{tid}/trigger", post(api::tasks::trigger_template))
        .route("/households/{id}/shopping", get(api::shopping::overview))
        .route("/households/{id}/lists", post(api::shopping::create_list))
        .route("/lists/{lid}", patch(api::shopping::rename_list).delete(api::shopping::delete_list))
        .route("/lists/{lid}/items", post(api::shopping::add_item))
        .route("/items/{iid}", patch(api::shopping::update_item).delete(api::shopping::delete_item))
        .route("/items/{iid}/check", post(api::shopping::check_item));

    Router::new()
        .route("/health", get(health))
        .nest("/api/v1", v1.layer(axum::middleware::from_fn_with_state(state.clone(), crate::idempotency::layer)))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

#[derive(Serialize)]
struct Health {
    status: &'static str,
    database: bool,
    version: &'static str,
}

/// Liveness + DB check for Caddy/Docker health checks and the app's connection screen.
async fn health(State(state): State<AppState>) -> Json<Health> {
    let database = sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&state.db).await.is_ok();
    Json(Health { status: if database { "ok" } else { "degraded" }, database, version: env!("CARGO_PKG_VERSION") })
}

#[derive(Serialize)]
struct Meta {
    version: &'static str,
    default_point_policy: PointPolicy,
    default_period_policy: PeriodPolicy,
}

/// Server version and defaults for new households (used by the app's setup wizard).
async fn meta() -> Json<Meta> {
    Json(Meta {
        version: env!("CARGO_PKG_VERSION"),
        default_point_policy: PointPolicy::default(),
        default_period_policy: PeriodPolicy::default(),
    })
}

#[derive(Deserialize)]
struct SettlementPreviewRequest {
    policy: PeriodPolicy,
    members: Vec<MemberPeriodInput>,
}

/// Stateless "what if" settlement: lets admins try out rules before saving them.
async fn preview_settlement(Json(req): Json<SettlementPreviewRequest>) -> AppResult<Json<Settlement>> {
    if req.members.is_empty() {
        return Err(AppError::BadRequest("Mindestens ein Mitglied angeben".into()));
    }
    if req.members.len() > 50 {
        return Err(AppError::BadRequest("Höchstens 50 Mitglieder".into()));
    }
    Ok(Json(punkte_core::settlement::settle(&req.policy, &req.members)))
}
