//! "Mit Google anmelden" in the desktop app (OAuth for installed apps, RFC 8252): open Google in
//! the browser, wait on a loopback port for the redirect with the authorization code (PKCE with
//! S256 and a random `state`), and hand code, verifier and redirect to the web view. The server
//! swaps the code for the ID token, because Google wants the client secret for that.

use std::collections::HashMap;
use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64;
use sha2::{Digest, Sha256};
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
/// Give up waiting for the browser after this long.
const WAIT: Duration = Duration::from_secs(300);

/// Set by `google_desktop_cancel` (the "Abbrechen" button) to stop waiting.
static CANCEL: AtomicBool = AtomicBool::new(false);

#[derive(serde::Serialize)]
pub struct GoogleCode {
    code: String,
    verifier: String,
    redirect_uri: String,
}

fn random(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::fill(&mut buf[..]);
    B64.encode(buf)
}

#[tauri::command]
pub async fn google_desktop_sign_in(
    app: tauri::AppHandle,
    client_id: String,
) -> Result<GoogleCode, String> {
    CANCEL.store(false, Ordering::SeqCst);
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    let redirect_uri = format!("http://127.0.0.1:{port}");
    let verifier = random(32);
    let challenge = B64.encode(Sha256::digest(verifier.as_bytes()));
    let state = random(16);

    let mut url = url::Url::parse(AUTH_URL).map_err(|e| e.to_string())?;
    url.query_pairs_mut()
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", "openid email profile")
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("prompt", "select_account");
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|e| e.to_string())?;

    let code = tauri::async_runtime::spawn_blocking(move || wait_for_code(listener, &state))
        .await
        .map_err(|e| e.to_string())??;
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.set_focus();
    }
    Ok(GoogleCode {
        code,
        verifier,
        redirect_uri,
    })
}

/// Stops a running `google_desktop_sign_in` (it then fails with "cancelled").
#[tauri::command]
pub fn google_desktop_cancel() {
    CANCEL.store(true, Ordering::SeqCst);
}

fn wait_for_code(listener: TcpListener, state: &str) -> Result<String, String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let deadline = Instant::now() + WAIT;
    loop {
        if CANCEL.load(Ordering::SeqCst) || Instant::now() > deadline {
            return Err("cancelled".into());
        }
        match listener.accept() {
            Ok((stream, _)) => {
                if let Some(result) = answer(stream, state) {
                    return result;
                }
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100))
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Reads one request. The redirect (path `/`) ends the wait with the code or an error; anything
/// else (a favicon request) gets a 404 and the wait goes on.
fn answer(mut stream: TcpStream, state: &str) -> Option<Result<String, String>> {
    stream.set_nonblocking(false).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).ok()?;
    let request = String::from_utf8_lossy(&buf[..n]);
    let target = request
        .lines()
        .next()?
        .split_whitespace()
        .nth(1)?
        .to_string();
    let url = url::Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    if url.path() != "/" {
        reply(&mut stream, "404 Not Found", "");
        return None;
    }
    let q: HashMap<String, String> = url.query_pairs().into_owned().collect();
    let result = if q.get("state").map(String::as_str) != Some(state) {
        Err(
            "Die Antwort von Google passt nicht zu dieser Anmeldung. Bitte noch einmal versuchen."
                .to_string(),
        )
    } else if let Some(error) = q.get("error") {
        Err(if error == "access_denied" {
            "cancelled".to_string()
        } else {
            format!("Google meldet: {error}")
        })
    } else if let Some(code) = q.get("code") {
        Ok(code.clone())
    } else {
        reply(&mut stream, "400 Bad Request", "");
        return None;
    };
    let (title, text) = match &result {
        Ok(_) => (
            "Angemeldet",
            "Du kannst dieses Fenster schließen und zu Fairteilt zurückkehren.",
        ),
        Err(_) => (
            "Nicht angemeldet",
            "Die Anmeldung mit Google hat nicht geklappt. Schließ dieses Fenster und versuch es in Fairteilt noch einmal.",
        ),
    };
    let page = format!(
        "<!doctype html><html lang=\"de\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width\">\
         <title>Fairteilt</title><body style=\"font-family:system-ui,sans-serif;background:#fbf7f0;color:#2b2622;\
         display:grid;place-items:center;min-height:90vh;margin:0\"><main style=\"max-width:28rem;padding:1rem;text-align:center\">\
         <h1 style=\"color:#4338ca\">{title}</h1><p>{text}</p></main></body></html>"
    );
    reply(&mut stream, "200 OK", &page);
    Some(result)
}

fn reply(stream: &mut TcpStream, status: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.flush();
}
