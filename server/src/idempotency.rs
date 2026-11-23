//! Idempotent writes for the offline queue (KONZEPT §9.2).
//!
//! The app sends every queued write with an `Idempotency-Key` header (a UUID it generated when
//! the user acted). If the request reached the server before but the answer got lost, the same
//! key gets the stored answer again instead of doing the work twice (no double XP, no second
//! transfer). Keys belong to the signed-in user and are kept for seven days.

use axum::body::{Body, to_bytes};
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use uuid::Uuid;

use crate::AppState;
use crate::auth::decode_access_token;

const HEADER: &str = "idempotency-key";
const MAX_BODY: usize = 1024 * 1024;

pub async fn layer(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if req.method() == Method::GET || req.method() == Method::OPTIONS {
        return next.run(req).await;
    }
    let key = req.headers().get(HEADER).and_then(|v| v.to_str().ok()).map(str::trim).filter(|k| !k.is_empty() && k.len() <= 100).map(str::to_string);
    let user = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .and_then(|t| decode_access_token(&state.auth, t))
        .map(|c| c.sub);
    let (Some(key), Some(user)) = (key, user) else {
        return next.run(req).await;
    };
    let (method, path) = (req.method().to_string(), req.uri().path().to_string());

    if let Ok(Some((m, p, status, body))) = sqlx::query_as::<_, (String, String, i16, Vec<u8>)>(
        "SELECT method, path, status, body FROM idempotency_keys WHERE user_id = $1 AND key = $2",
    )
    .bind(user)
    .bind(&key)
    .fetch_optional(&state.db)
    .await
    {
        if m != method || p != path {
            return (StatusCode::UNPROCESSABLE_ENTITY, "Idempotency-Key was used for another request").into_response();
        }
        return stored(status, body);
    }

    let res = next.run(req).await;
    let status = res.status();
    // Remember answers that will not change on retry: success and the client's own mistakes
    // (not 401 "token expired" or 429 "too many", those are worth another try).
    if !(status.is_success() || status.is_client_error()) || status == StatusCode::UNAUTHORIZED || status == StatusCode::TOO_MANY_REQUESTS {
        return res;
    }
    let (parts, body) = res.into_parts();
    let Ok(bytes) = to_bytes(body, MAX_BODY).await else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    let saved = sqlx::query(
        "INSERT INTO idempotency_keys (user_id, key, method, path, status, body) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (user_id, key) DO NOTHING",
    )
    .bind(user)
    .bind(&key)
    .bind(&method)
    .bind(&path)
    .bind(status.as_u16() as i16)
    .bind(bytes.to_vec())
    .execute(&state.db)
    .await;
    if let Err(err) = saved {
        tracing::warn!(%err, "could not store idempotency key");
    }
    Response::from_parts(parts, Body::from(bytes))
}

fn stored(status: i16, body: Vec<u8>) -> Response {
    let mut res = Response::new(Body::from(body));
    *res.status_mut() = StatusCode::from_u16(status as u16).unwrap_or(StatusCode::OK);
    res.headers_mut().insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    res.headers_mut().insert("idempotent-replay", HeaderValue::from_static("true"));
    res
}

/// Old keys are dropped by the reminder sweep.
pub async fn prune(db: &sqlx::PgPool) -> sqlx::Result<u64> {
    Ok(sqlx::query("DELETE FROM idempotency_keys WHERE created_at < now() - interval '7 days'").execute(db).await?.rows_affected())
}

/// For tests: a fresh key.
pub fn new_key() -> String {
    Uuid::new_v4().to_string()
}
