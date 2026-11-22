//! Small in-memory rate limiter (fixed window per key) and the client-key extractor.
//!
//! Good enough for a single API instance; with several instances this would move
//! to Postgres or Redis. PIN brute force is additionally stopped by the per-profile
//! lockout in the database.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;

use crate::AppState;
use crate::error::AppError;

#[derive(Default)]
pub struct RateLimiter {
    windows: Mutex<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    /// Counts one hit for `key`; `Err` when more than `limit` hits fall into `window`.
    pub fn check(&self, key: &str, limit: u32, window: Duration) -> Result<(), AppError> {
        let now = Instant::now();
        let mut map = self.windows.lock().expect("rate limiter poisoned");
        if map.len() > 50_000 {
            map.retain(|_, (start, _)| now.duration_since(*start) < Duration::from_secs(3600));
        }
        let entry = map.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) >= window {
            *entry = (now, 0);
        }
        entry.1 += 1;
        if entry.1 > limit { Err(AppError::TooManyRequests) } else { Ok(()) }
    }
}

/// Who is calling, for rate limiting: the first `X-Forwarded-For` hop when the server
/// runs behind a trusted proxy (`TRUST_PROXY=1`, e.g. Caddy), else the socket address.
#[derive(Debug, Clone)]
pub struct ClientKey(pub String);

impl FromRequestParts<AppState> for ClientKey {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        if state.trust_proxy
            && let Some(ip) = parts
                .headers
                .get("x-forwarded-for")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.split(',').next())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        {
            return Ok(ClientKey(ip));
        }
        let ip = parts
            .extensions
            .get::<ConnectInfo<SocketAddr>>()
            .map(|ci| ci.0.ip().to_string())
            .unwrap_or_else(|| "local".into());
        Ok(ClientKey(ip))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_per_key_and_resets() {
        let rl = RateLimiter::default();
        for _ in 0..3 {
            assert!(rl.check("a", 3, Duration::from_secs(60)).is_ok());
        }
        assert!(rl.check("a", 3, Duration::from_secs(60)).is_err(), "4th hit is over the limit");
        assert!(rl.check("b", 3, Duration::from_secs(60)).is_ok(), "other keys are independent");
        assert!(rl.check("c", 1, Duration::from_millis(1)).is_ok());
        std::thread::sleep(Duration::from_millis(5));
        assert!(rl.check("c", 1, Duration::from_millis(1)).is_ok(), "window expired, counter reset");
    }
}
