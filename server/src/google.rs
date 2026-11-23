//! Sign in with Google: the app gets an ID token from Google (Credential Manager on Android,
//! Google Identity Services in the browser) and sends it here. The token is checked against
//! Google's public keys: signature (RS256), issuer, audience (our OAuth client IDs), expiry and
//! a verified e-mail address. Keys are cached and fetched again when an unknown key id shows up.
//!
//! The desktop app cannot get an ID token directly: it opens Google in the browser and gets an
//! authorization code back on a loopback address (with PKCE). Swapping that code needs the
//! secret of the "Desktop" OAuth client, so the server does it (`exchange`).

use std::time::{Duration, Instant};

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::error::{AppError, AppResult};

const CERTS_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
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

/// The "Desktop" OAuth client (ID and secret) for the code exchange of the desktop app.
#[derive(Clone)]
pub struct DesktopClient {
    pub id: String,
    pub secret: String,
}

impl std::fmt::Debug for DesktopClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopClient")
            .field("id", &self.id)
            .field("secret", &"…")
            .finish()
    }
}

pub struct GoogleAuth {
    /// OAuth client IDs whose tokens we accept (the web client ID; Android asks Google for a
    /// token for that same ID; the desktop client ID). Empty = Google sign-in is off.
    client_ids: Vec<String>,
    desktop: Option<DesktopClient>,
    http: reqwest::Client,
    keys: RwLock<Option<(Instant, Vec<GoogleKey>)>>,
    /// Tests put their own keys here and never go to the network.
    fixed: bool,
}

impl GoogleAuth {
    pub fn new(client_ids: Vec<String>) -> Self {
        Self {
            client_ids,
            desktop: None,
            http: reqwest::Client::new(),
            keys: RwLock::new(None),
            fixed: false,
        }
    }

    /// Also sign in from the desktop app: its tokens are accepted and its codes swapped.
    pub fn with_desktop(mut self, desktop: DesktopClient) -> Self {
        if !self.client_ids.contains(&desktop.id) {
            self.client_ids.push(desktop.id.clone());
        }
        self.desktop = Some(desktop);
        self
    }

    /// For tests: accept tokens for `client_ids`, signed with `keys`.
    pub fn with_keys(client_ids: Vec<String>, keys: Vec<GoogleKey>) -> Self {
        Self {
            client_ids,
            desktop: None,
            http: reqwest::Client::new(),
            keys: RwLock::new(Some((Instant::now(), keys))),
            fixed: true,
        }
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

    /// Desktop app: swaps the authorization code from the browser for an ID token. The redirect
    /// has to be the loopback address the app listened on; Google checks it against the code.
    pub async fn exchange(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> AppResult<String> {
        let Some(desktop) = &self.desktop else {
            return Err(AppError::BadRequest(
                "Die Anmeldung mit Google am Computer ist auf diesem Server nicht eingerichtet."
                    .into(),
            ));
        };
        if !is_loopback(redirect_uri) || code.is_empty() || !(43..=128).contains(&verifier.len()) {
            return Err(rejected());
        }
        let body = form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", redirect_uri),
            ("client_id", &desktop.id),
            ("client_secret", &desktop.secret),
        ]);
        let res = self
            .http
            .post(TOKEN_URL)
            .header(
                reqwest::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .body(body)
            .timeout(Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!("Google code exchange: {e}")))?;
        if !res.status().is_success() {
            tracing::info!(status = %res.status(), "Google refused the code exchange");
            return Err(rejected());
        }
        #[derive(Deserialize)]
        struct Tokens {
            id_token: String,
        }
        let tokens: Tokens = res.json().await.map_err(|_| rejected())?;
        Ok(tokens.id_token)
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

/// `http://127.0.0.1:<port>` (optionally with a trailing slash), where the desktop app listens.
pub fn is_loopback(uri: &str) -> bool {
    uri.strip_prefix("http://127.0.0.1:")
        .map(|rest| rest.strip_suffix('/').unwrap_or(rest))
        .is_some_and(|port| port.parse::<u16>().is_ok_and(|p| p > 0))
}

/// `application/x-www-form-urlencoded` body.
fn form(pairs: &[(&str, &str)]) -> String {
    fn enc(s: &str) -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                    (b as char).to_string()
                }
                _ => format!("%{b:02X}"),
            })
            .collect()
    }
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", enc(k), enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn rejected() -> AppError {
    AppError::BadRequest("Die Anmeldung mit Google hat nicht geklappt. Bitte noch einmal versuchen.".into())
}
