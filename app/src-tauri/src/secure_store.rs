//! Secure storage for the session (refresh token, server address).
//!
//! - Windows, macOS, iOS, Linux: the OS keychain via `keyring`
//!   (Credential Manager, Keychain, kernel keyring).
//! - Android: a file in the app-private data directory, which only Fairteilt can read
//!   (Android sandbox). A Keystore-backed variant can replace it later.
//!
//! The short-lived access token never leaves memory in the webview.

use tauri::AppHandle;

const SERVICE: &str = "eu.olitech.fairteilt";

#[cfg(not(target_os = "android"))]
mod imp {
    use super::SERVICE;
    use keyring::Entry;

    fn entry(key: &str) -> Result<Entry, String> {
        Entry::new(SERVICE, key).map_err(|e| e.to_string())
    }

    pub fn set(_app: &tauri::AppHandle, key: &str, value: &str) -> Result<(), String> {
        entry(key)?.set_password(value).map_err(|e| e.to_string())
    }

    pub fn get(_app: &tauri::AppHandle, key: &str) -> Result<Option<String>, String> {
        match entry(key)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn delete(_app: &tauri::AppHandle, key: &str) -> Result<(), String> {
        match entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg(target_os = "android")]
mod imp {
    use std::path::PathBuf;
    use tauri::Manager;

    fn path(app: &tauri::AppHandle, key: &str) -> Result<PathBuf, String> {
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("secure");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let safe: String = key.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        Ok(dir.join(format!("{safe}.txt")))
    }

    pub fn set(app: &tauri::AppHandle, key: &str, value: &str) -> Result<(), String> {
        std::fs::write(path(app, key)?, value).map_err(|e| e.to_string())
    }

    pub fn get(app: &tauri::AppHandle, key: &str) -> Result<Option<String>, String> {
        match std::fs::read_to_string(path(app, key)?) {
            Ok(v) => Ok(Some(v)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn delete(app: &tauri::AppHandle, key: &str) -> Result<(), String> {
        match std::fs::remove_file(path(app, key)?) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn check_key(key: &str) -> Result<(), String> {
    if key.is_empty() || key.len() > 64 || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err("invalid key".into());
    }
    Ok(())
}

#[tauri::command]
pub fn secure_set(app: AppHandle, key: String, value: String) -> Result<(), String> {
    check_key(&key)?;
    imp::set(&app, &key, &value)
}

#[tauri::command]
pub fn secure_get(app: AppHandle, key: String) -> Result<Option<String>, String> {
    check_key(&key)?;
    imp::get(&app, &key)
}

#[tauri::command]
pub fn secure_delete(app: AppHandle, key: String) -> Result<(), String> {
    check_key(&key)?;
    imp::delete(&app, &key)
}
