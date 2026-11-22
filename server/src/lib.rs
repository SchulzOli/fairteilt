//! punkte-server as a library: router, state and handlers. `main.rs` only wires up
//! config, database and the scheduler; integration tests use the router directly.

pub mod api;
pub mod auth;
pub mod config;
pub mod error;
pub mod live;
pub mod ratelimit;
pub mod routes;
pub mod scheduler;

use std::sync::Arc;

/// Shared state for all handlers.
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub auth: Arc<auth::AuthKeys>,
    pub limiter: Arc<ratelimit::RateLimiter>,
    pub live: Arc<live::Hub>,
    /// Take the client address from `X-Forwarded-For` (only behind a trusted proxy).
    pub trust_proxy: bool,
}

impl AppState {
    pub fn new(db: sqlx::PgPool, jwt_secret: &[u8]) -> Self {
        Self {
            db,
            auth: Arc::new(auth::AuthKeys::new(jwt_secret)),
            limiter: Arc::new(ratelimit::RateLimiter::default()),
            live: Arc::new(live::Hub::default()),
            trust_proxy: false,
        }
    }
}

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");
