//! Push over Firebase Cloud Messaging, HTTP v1 API (KONZEPT §8.2, stage 2).
//!
//! Configured with a Google service account (`FCM_SERVICE_ACCOUNT` = path to its JSON file).
//! Without it the server runs fine: inbox and live updates work, nothing is pushed.

use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use serde::Deserialize;
use tokio::sync::Mutex;

const SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";
const GRANT: &str = "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion=";

enum Auth {
    ServiceAccount { client_email: String, key: EncodingKey, token_uri: String },
    /// Fixed access token (tests against a local stand-in for FCM).
    Static(String),
}

pub struct Fcm {
    http: reqwest::Client,
    /// `https://fcm.googleapis.com/v1/projects/{project}/messages:send`
    endpoint: String,
    auth: Auth,
    cached: Mutex<Option<(String, Instant)>>,
}

/// What happened to one message.
#[derive(Debug, PartialEq, Eq)]
pub enum Delivery {
    Delivered,
    /// The token is gone (app uninstalled, token rotated): forget the device.
    Unregistered,
}

#[derive(Deserialize)]
struct ServiceAccount {
    project_id: String,
    client_email: String,
    private_key: String,
    token_uri: String,
}

#[derive(serde::Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

impl Fcm {
    pub fn from_service_account_file(path: &str) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path).with_context(|| format!("reading {path}"))?;
        let sa: ServiceAccount = serde_json::from_str(&raw).context("service account JSON")?;
        let key = EncodingKey::from_rsa_pem(sa.private_key.as_bytes()).context("service account private key")?;
        Ok(Self {
            http: client()?,
            endpoint: format!("https://fcm.googleapis.com/v1/projects/{}/messages:send", sa.project_id),
            auth: Auth::ServiceAccount { client_email: sa.client_email, key, token_uri: sa.token_uri },
            cached: Mutex::new(None),
        })
    }

    /// For tests: send to `endpoint` with a fixed bearer token.
    pub fn with_static_token(endpoint: String, token: String) -> anyhow::Result<Self> {
        Ok(Self { http: client()?, endpoint, auth: Auth::Static(token), cached: Mutex::new(None) })
    }

    async fn access_token(&self) -> anyhow::Result<String> {
        let (client_email, key, token_uri) = match &self.auth {
            Auth::Static(t) => return Ok(t.clone()),
            Auth::ServiceAccount { client_email, key, token_uri } => (client_email, key, token_uri),
        };
        let mut cached = self.cached.lock().await;
        if let Some((token, until)) = cached.as_ref()
            && *until > Instant::now()
        {
            return Ok(token.clone());
        }
        let iat = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs();
        let claims = Claims { iss: client_email, scope: SCOPE, aud: token_uri, iat, exp: iat + 3600 };
        let jwt = jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, key)?;
        let res = self
            .http
            .post(token_uri)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(format!("{GRANT}{jwt}"))
            .send()
            .await?;
        if !res.status().is_success() {
            bail!("token endpoint answered {}", res.status());
        }
        let t: TokenResponse = res.json().await?;
        // Renew a minute early.
        let until = Instant::now() + Duration::from_secs(t.expires_in.saturating_sub(60));
        *cached = Some((t.access_token.clone(), until));
        Ok(t.access_token)
    }

    /// Sends one notification to one device. `data` arrives in the app (where to open).
    pub async fn send(&self, device_token: &str, title: &str, body: Option<&str>, data: serde_json::Value) -> anyhow::Result<Delivery> {
        let token = self.access_token().await?;
        let message = serde_json::json!({
            "message": {
                "token": device_token,
                "notification": { "title": title, "body": body.unwrap_or("") },
                "data": data,
                "android": { "priority": "high", "notification": { "channel_id": "fairteilt" } },
            }
        });
        let res = self.http.post(&self.endpoint).bearer_auth(token).json(&message).send().await?;
        let status = res.status();
        if status.is_success() {
            return Ok(Delivery::Delivered);
        }
        let text = res.text().await.unwrap_or_default();
        // FCM: 404 UNREGISTERED, or 400 INVALID_ARGUMENT for a malformed token.
        if status.as_u16() == 404 || text.contains("UNREGISTERED") || (status.as_u16() == 400 && text.contains("registration token")) {
            return Ok(Delivery::Unregistered);
        }
        bail!("FCM answered {status}: {}", text.chars().take(200).collect::<String>())
    }
}

fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder().timeout(Duration::from_secs(15)).build()?)
}
