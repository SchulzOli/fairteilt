use anyhow::{Context, bail};

/// Runtime configuration from environment variables (see `deploy/.env.example`).
#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: String,
    /// Allowed browser origins for CORS (dev server, Tauri webview). Comma-separated.
    pub cors_origins: Vec<String>,
    /// Secret for signing access tokens (at least 32 characters).
    pub jwt_secret: String,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let database_url = std::env::var("DATABASE_URL").context("DATABASE_URL is not set")?;
        let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
        let cors_origins = std::env::var("CORS_ORIGINS")
            .unwrap_or_else(|_| {
                // Vite dev server + Tauri webview origins (Windows/Android use http(s)://tauri.localhost,
                // macOS/iOS/Linux use tauri://localhost).
                "http://localhost:1420,http://tauri.localhost,https://tauri.localhost,tauri://localhost".into()
            })
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let jwt_secret = std::env::var("JWT_SECRET").context("JWT_SECRET is not set")?;
        if jwt_secret.len() < 32 {
            bail!("JWT_SECRET must be at least 32 characters (e.g. `openssl rand -base64 48`)");
        }
        Ok(Self { database_url, bind_addr, cors_origins, jwt_secret })
    }
}
