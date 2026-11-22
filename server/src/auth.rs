//! Passwords, tokens and the authenticated-user extractor.
//!
//! - Passwords: Argon2id (PHC string in `users.password_hash`).
//! - Access token: short-lived JWT (HS256) with user id and session id.
//! - Refresh token: 32 random bytes, stored only as SHA-256 hash in `sessions`,
//!   rotated on every refresh, revocable per device.

use std::time::Duration;

use argon2::Argon2;
use argon2::password_hash::phc::PasswordHash;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::AppState;
use crate::error::AppError;

pub const ACCESS_TTL: Duration = Duration::from_secs(15 * 60);
pub const REFRESH_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Secrets for signing access tokens.
#[derive(Clone)]
pub struct AuthKeys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl AuthKeys {
    pub fn new(secret: &[u8]) -> Self {
        Self { encoding: EncodingKey::from_secret(secret), decoding: DecodingKey::from_secret(secret) }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// User id
    pub sub: Uuid,
    /// Session id (one per device)
    pub sid: Uuid,
    pub iat: i64,
    pub exp: i64,
}

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let hash = Argon2::default()
        .hash_password(password.as_bytes())
        .map_err(|e| anyhow::anyhow!("hashing password: {e}"))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, phc: &str) -> bool {
    match PasswordHash::new(phc) {
        Ok(parsed) => Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok(),
        Err(_) => false,
    }
}

/// A valid hash of a random password, verified against when the user does not exist,
/// so that login takes the same time either way (no user enumeration by timing).
pub fn dummy_hash() -> &'static str {
    use std::sync::OnceLock;
    static DUMMY: OnceLock<String> = OnceLock::new();
    DUMMY.get_or_init(|| hash_password(&hex::encode(random_bytes::<16>())).expect("dummy hash"))
}

pub fn issue_access_token(keys: &AuthKeys, user_id: Uuid, session_id: Uuid) -> anyhow::Result<String> {
    let now = Utc::now().timestamp();
    let claims = Claims { sub: user_id, sid: session_id, iat: now, exp: now + ACCESS_TTL.as_secs() as i64 };
    Ok(jsonwebtoken::encode(&Header::new(Algorithm::HS256), &claims, &keys.encoding)?)
}

pub fn decode_access_token(keys: &AuthKeys, token: &str) -> Option<Claims> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 5;
    jsonwebtoken::decode::<Claims>(token, &keys.decoding, &validation).ok().map(|d| d.claims)
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0u8; N];
    rand::fill(&mut bytes);
    bytes
}

/// SHA-256 hex of a secret token (refresh token, invite code). Only hashes are stored.
pub fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// New refresh token and its hash.
pub fn new_refresh_token() -> (String, String) {
    let token = hex::encode(random_bytes::<32>());
    let hash = hash_token(&token);
    (token, hash)
}

/// Invite code like `K7QF-M2XA`: 8 characters without look-alikes (0/O, 1/I/L).
pub fn new_invite_code() -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
    let bytes = random_bytes::<8>();
    let chars: String = bytes.iter().map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char).collect();
    format!("{}-{}", &chars[..4], &chars[4..])
}

/// Normalizes user input of an invite code (case, spaces, missing dash).
pub fn normalize_invite_code(input: &str) -> String {
    let compact: String = input.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase();
    if compact.len() == 8 { format!("{}-{}", &compact[..4], &compact[4..]) } else { compact }
}

/// The authenticated caller. Requires `Authorization: Bearer <access token>` of a session
/// that has not been revoked.
#[derive(Debug, Clone, Copy)]
pub struct AuthUser {
    pub user_id: Uuid,
    pub session_id: Uuid,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let token = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(AppError::Unauthorized)?;
        let claims = decode_access_token(&state.auth, token).ok_or(AppError::Unauthorized)?;
        let active: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM sessions WHERE id = $1 AND user_id = $2 AND revoked_at IS NULL AND expires_at > now())",
        )
        .bind(claims.sid)
        .bind(claims.sub)
        .fetch_one(&state.db)
        .await?;
        if !active {
            return Err(AppError::Unauthorized);
        }
        Ok(AuthUser { user_id: claims.sub, session_id: claims.sid })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let h = hash_password("korrekt pferd batterie").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("korrekt pferd batterie", &h));
        assert!(!verify_password("falsch", &h));
        assert!(!verify_password("x", "not a phc string"));
    }

    #[test]
    fn access_token_roundtrip_and_tamper() {
        let keys = AuthKeys::new(b"0123456789abcdef0123456789abcdef");
        let (u, s) = (Uuid::new_v4(), Uuid::new_v4());
        let t = issue_access_token(&keys, u, s).unwrap();
        let c = decode_access_token(&keys, &t).unwrap();
        assert_eq!((c.sub, c.sid), (u, s));
        let other = AuthKeys::new(b"another-secret-another-secret-xx");
        assert!(decode_access_token(&other, &t).is_none(), "wrong key must fail");
        assert!(decode_access_token(&keys, &format!("{t}x")).is_none(), "tampered token must fail");
    }

    #[test]
    fn refresh_tokens_are_random_and_hashed() {
        let (a, ha) = new_refresh_token();
        let (b, _) = new_refresh_token();
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
        assert_eq!(ha, hash_token(&a));
        assert_ne!(ha, a);
    }

    #[test]
    fn invite_codes() {
        let c = new_invite_code();
        assert_eq!(c.len(), 9);
        assert_eq!(&c[4..5], "-");
        assert!(!c.contains(['0', 'O', '1', 'I', 'L']));
        assert_eq!(normalize_invite_code(" k7qf m2xa "), "K7QF-M2XA");
        assert_eq!(normalize_invite_code("K7QF-M2XA"), "K7QF-M2XA");
    }
}
