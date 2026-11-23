//! Sign in with Google: the app gets an ID token from Google (Credential Manager on Android,
//! Google Identity Services in the browser) and sends it here. The token is checked against
//! Google's public keys: signature (RS256), issuer, audience (our OAuth client IDs), expiry and
//! a verified e-mail address. Keys are cached and fetched again when an unknown key id shows up.

use std::time::{Duration, Instant};

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::error::{AppError, AppResult};

const CERTS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
const ISSUERS: [&str; 2] = ["accounts.google.com", "https://accounts.google.com"];
/// Google rotates its keys about every few days; fetch again at least this often.
const KEYS_MAX_AGE: Duration = Duration::from_secs(6 * 3600);

/// One RSA public key of Google (JWK).
#[derive(Deserialize, Clone, Debug)]
pub struct GoogleKey {
    pub kid: String,
    pub n: String,
    pub e: String,
}

#[derive(Deserialize)]
struct KeySet {
    keys: Vec<GoogleKey>,
}

/// What we use from the ID token.
#[derive(Deserialize, Debug, Clone)]
pub struct GoogleClaims {
    /// Google's stable id for the account.
    pub sub: String,
    pub email: Option<String>,
    #[serde(default)]
    pub email_verified: bool,
    pub name: Option<String>,
    pub given_name: Option<String>,
}

pub struct GoogleAuth {
    /// OAuth client IDs whose tokens we accept (the web client ID; Android asks Google for a
    /// token for that same ID). Empty = Google sign-in is off.
    client_ids: Vec<String>,
    http: reqwest::Client,
    keys: RwLock<Option<(Instant, Vec<GoogleKey>)>>,
    /// Tests put their own keys here and never go to the network.
    fixed: bool,
}

impl GoogleAuth {
    pub fn new(client_ids: Vec<String>) -> Self {
        Self { client_ids, http: reqwest::Client::new(), keys: RwLock::new(None), fixed: false }
    }

    /// For tests: accept tokens for `client_ids`, signed with `keys`.
    pub fn with_keys(client_ids: Vec<String>, keys: Vec<GoogleKey>) -> Self {
        Self { client_ids, http: reqwest::Client::new(), keys: RwLock::new(Some((Instant::now(), keys))), fixed: true }
    }

    pub fn enabled(&self) -> bool {
        !self.client_ids.is_empty()
    }

    async fn key(&self, kid: &str) -> AppResult<GoogleKey> {
        if let Some((at, keys)) = &*self.keys.read().await
            && (self.fixed || at.elapsed() < KEYS_MAX_AGE)
            && let Some(k) = keys.iter().find(|k| k.kid == kid)
        {
            return Ok(k.clone());
        }
        if self.fixed {
            return Err(rejected());
        }
        let set: KeySet = self
            .http
            .get(CERTS_URL)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| AppError::Other(anyhow::anyhow!("fetching Google keys: {e}")))?
            .json()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!("reading Google keys: {e}")))?;
        let found = set.keys.iter().find(|k| k.kid == kid).cloned();
        *self.keys.write().await = Some((Instant::now(), set.keys));
        found.ok_or_else(rejected)
    }

    /// Checks an ID token and returns its claims.
    pub async fn verify(&self, id_token: &str) -> AppResult<GoogleClaims> {
        if !self.enabled() {
            return Err(AppError::BadRequest("Die Anmeldung mit Google ist auf diesem Server nicht eingerichtet.".into()));
        }
        let header = decode_header(id_token).map_err(|_| rejected())?;
        if header.alg != Algorithm::RS256 {
            return Err(rejected());
        }
        let kid = header.kid.ok_or_else(rejected)?;
        let key = self.key(&kid).await?;
        let decoding = DecodingKey::from_rsa_components(&key.n, &key.e).map_err(|_| rejected())?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&self.client_ids);
        validation.set_issuer(&ISSUERS);
        validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
        let claims = decode::<GoogleClaims>(id_token, &decoding, &validation).map_err(|_| rejected())?.claims;
        if !claims.email_verified || claims.email.as_deref().is_none_or(str::is_empty) {
            return Err(AppError::BadRequest("Dieses Google-Konto hat keine bestätigte E-Mail-Adresse.".into()));
        }
        Ok(claims)
    }
}

fn rejected() -> AppError {
    AppError::BadRequest("Die Anmeldung mit Google hat nicht geklappt. Bitte noch einmal versuchen.".into())
}
