//! HTTP routes. Everything under `/api/v1`.

use axum::extract::State;
use axum::http::{HeaderValue, Method, header};
use axum::routing::{get, post};
use axum::{Json, Router};
use punkte_core::{MemberPeriodInput, PeriodPolicy, PointPolicy, Settlement};
use serde::{Deserialize, Serialize};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::AppState;
use crate::config::Config;
use crate::error::{AppError, AppResult};

pub fn router(state: AppState, config: &Config) -> Router {
    let origins: Vec<HeaderValue> = config.cors_origins.iter().filter_map(|o| o.parse().ok()).collect();
    let cors = CorsLayer::new()
        .allow_origin(origins)
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::PATCH, Method::DELETE])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let api = Router::new()
        .route("/meta", get(meta))
        .route("/preview/settlement", post(preview_settlement));

    Router::new()
        .route("/health", get(health))
        .nest("/api/v1", api)
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
