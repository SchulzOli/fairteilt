//! Tauri core of the Fairteilt app.
//!
//! The Rust side owns everything that should not live in the webview: talking to the
//! server (no CORS / mixed-content issues), later the offline outbox, the local SQLite
//! cache and the token in the OS keychain (see docs/KONZEPT.md §9). It also links
//! `punkte-core`, so rules can be previewed without a connection.

use std::time::Duration;

use punkte_core::settlement::settle;
use punkte_core::{MemberPeriodInput, PeriodPolicy, PointPolicy, Settlement};
use serde::{Deserialize, Serialize};

/// Error shape returned to the frontend.
#[derive(Debug, Serialize)]
struct CommandError {
    message: String,
}

impl<E: std::fmt::Display> From<E> for CommandError {
    fn from(err: E) -> Self {
        Self { message: err.to_string() }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct ServerHealth {
    status: String,
    database: bool,
    version: String,
}

/// Checks the connection to the punkte-server at `base_url` (e.g. https://punkte.example.org).
#[tauri::command]
async fn server_health(base_url: String) -> Result<ServerHealth, CommandError> {
    let url = format!("{}/health", base_url.trim_end_matches('/'));
    let client = reqwest::Client::builder().timeout(Duration::from_secs(8)).build()?;
    let health = client.get(&url).send().await?.error_for_status()?.json::<ServerHealth>().await?;
    Ok(health)
}

#[derive(Debug, Serialize)]
struct Defaults {
    point_policy: PointPolicy,
    period_policy: PeriodPolicy,
}

/// Default rules for a new household, straight from punkte-core (works offline).
#[tauri::command]
fn default_policies() -> Defaults {
    Defaults { point_policy: PointPolicy::default(), period_policy: PeriodPolicy::default() }
}

/// Offline settlement preview with exactly the server's algorithm.
#[tauri::command]
fn preview_settlement(policy: PeriodPolicy, members: Vec<MemberPeriodInput>) -> Settlement {
    settle(&policy, &members)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![server_health, default_policies, preview_settlement])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
