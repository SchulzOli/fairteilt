//! Tauri core of the Fairteilt app.
//!
//! The Rust side owns everything that should not live in the webview: talking to the
//! server (no CORS / mixed-content issues), later the offline outbox, the local SQLite
//! cache and the token in the OS keychain (see docs/KONZEPT.md §9). It also links
//! `punkte-core`, so rules can be previewed without a connection.

mod secure_store;

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

/// FCM push token of this device (Android writes it to `dataDir/fcm_token`, see Push.kt).
/// None elsewhere or before Firebase delivered one.
#[tauri::command]
fn push_token(app: tauri::AppHandle) -> Option<String> {
    #[cfg(target_os = "android")]
    {
        use tauri::Manager;
        let dir = app.path().app_data_dir().ok()?;
        let candidates = [dir.join("fcm_token"), dir.parent().map(|p| p.join("fcm_token")).unwrap_or_default()];
        candidates.iter().find_map(|p| std::fs::read_to_string(p).ok()).map(|t| t.trim().to_string()).filter(|t| !t.is_empty())
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = app;
        None
    }
}

/// Writes an export into the downloads folder (Android: the app's own downloads folder).
#[tauri::command]
fn save_file(app: tauri::AppHandle, name: String, content: String) -> Result<String, String> {
    use tauri::Manager;
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
    let dir = app.path().download_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(safe);
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// The app's own plugin for signing in with Google; the command lives in Kotlin.
#[cfg(target_os = "android")]
fn google_signin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("google-signin")
        .setup(|_app, api| {
            api.register_android_plugin("eu.olitech.fairteilt", "GoogleSignInPlugin")?;
            Ok(())
        })
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        // Local reminders (bald fällig, check-in with buttons).
        .plugin(tauri_plugin_notification::init());
    // QR scanning (join a household by code) exists on phones only.
    #[cfg(mobile)]
    let builder = builder.plugin(tauri_plugin_barcode_scanner::init());
    // "Mit Google anmelden" through Android's Credential Manager (GoogleSignIn.kt).
    #[cfg(target_os = "android")]
    let builder = builder.plugin(google_signin());
    builder
        .invoke_handler(tauri::generate_handler![
            server_health,
            default_policies,
            preview_settlement,
            push_token,
            save_file,
            secure_store::secure_set,
            secure_store::secure_get,
            secure_store::secure_delete
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
