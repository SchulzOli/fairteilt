//! Sign in with Google. Tokens here are signed with a test-only key (tests/fixtures), never
//! Google's: the server is given that key instead of fetching Google's.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use punkte_server::google::{GoogleAuth, GoogleKey};
use punkte_server::{AppState, routes};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const SECRET: &[u8] = b"test-secret-test-secret-test-secret!!";
const CLIENT: &str = "test-client.apps.googleusercontent.com";
const PEM: &str = include_str!("fixtures/google-test-key.pem");
const MODULUS: &str = include_str!("fixtures/google-test-key.n");

async fn call(app: &Router, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
    let mut req = Request::builder().method(method).uri(path);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap() };
    (status, json)
}

fn app(db: PgPool) -> Router {
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let mut state = AppState::new(db, SECRET);
    state.google = Arc::new(GoogleAuth::with_keys(
        vec![CLIENT.into()],
        vec![GoogleKey { kid: "test-key".into(), n: MODULUS.trim().into(), e: "AQAB".into() }],
    ));
    routes::router(state, &[])
}

/// An ID token as Google would issue it, with `overrides` on top.
fn token(sub: &str, email: &str, overrides: Value) -> String {
    let now = chrono::Utc::now().timestamp();
    let mut claims = json!({
        "iss": "https://accounts.google.com", "aud": CLIENT, "sub": sub, "email": email, "email_verified": true,
        "name": "Robin Beispiel", "given_name": "Robin", "iat": now, "exp": now + 3600
    });
    for (k, v) in overrides.as_object().unwrap() {
        claims[k] = v.clone();
    }
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some("test-key".into());
    encode(&header, &claims, &EncodingKey::from_rsa_pem(PEM.as_bytes()).unwrap()).unwrap()
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn sign_in_with_google(db: PgPool) {
    let app = app(db);
    let google = "/api/v1/auth/google";

    // First time: a new account, named after the Google account, without a password.
    let (s, b) = call(&app, "POST", google, None, Some(json!({ "id_token": token("g-1", "Robin@Example.org", json!({})), "device_name": "Pixel" }))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    assert_eq!((b["user"]["display_name"].as_str(), b["user"]["email"].as_str()), (Some("Robin"), Some("robin@example.org")));
    let first = b["user"]["id"].clone();
    let (s, me) = call(&app, "GET", "/api/v1/me", Some(b["access_token"].as_str().unwrap()), None).await;
    assert_eq!((s, me["user"]["id"].clone()), (StatusCode::OK, first.clone()), "{me}");

    // Again: the same account.
    let (s, b) = call(&app, "POST", google, None, Some(json!({ "id_token": token("g-1", "robin@example.org", json!({})) }))).await;
    assert_eq!((s, b["user"]["id"].clone()), (StatusCode::OK, first));

    // Someone who registered with e-mail and password: linked on the first Google sign-in.
    let (s, reg) = call(&app, "POST", "/api/v1/auth/register", None,
        Some(json!({ "email": "sam@example.org", "password": "sehr-geheim-123", "display_name": "Sam" }))).await;
    assert_eq!(s, StatusCode::CREATED);
    let (s, b) = call(&app, "POST", google, None, Some(json!({ "id_token": token("g-2", "sam@example.org", json!({})) }))).await;
    assert_eq!((s, b["user"]["id"].clone(), b["user"]["display_name"].as_str()), (StatusCode::OK, reg["user"]["id"].clone(), Some("Sam")));
    // The password still works.
    let (s, _) = call(&app, "POST", "/api/v1/auth/login", None, Some(json!({ "email": "sam@example.org", "password": "sehr-geheim-123" }))).await;
    assert_eq!(s, StatusCode::OK);

    // Rejected: other app, expired, unverified e-mail, wrong signature, nonsense.
    for bad in [
        token("g-3", "x@example.org", json!({ "aud": "someone-else" })),
        token("g-3", "x@example.org", json!({ "exp": chrono::Utc::now().timestamp() - 600 })),
        token("g-3", "x@example.org", json!({ "iss": "https://evil.example" })),
        token("g-3", "x@example.org", json!({ "email_verified": false })),
        format!("{}x", token("g-3", "x@example.org", json!({}))),
        "kein-token".to_string(),
    ] {
        let (s, b) = call(&app, "POST", google, None, Some(json!({ "id_token": bad }))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{b}");
    }
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn google_is_off_without_client_ids(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let (s, b) = call(&app, "POST", "/api/v1/auth/google", None, Some(json!({ "id_token": token("g-1", "a@example.org", json!({})) }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(b["error"].as_str().unwrap_or_default().contains("nicht eingerichtet") || b.to_string().contains("nicht eingerichtet"), "{b}");
}

/// Desktop app: an authorization code instead of an ID token. Without a desktop client on the
/// server it is refused; with one, only a loopback redirect is swapped (checked before any call
/// to Google). The redirect check itself is in `is_loopback`.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn desktop_code_needs_client_and_loopback(db: PgPool) {
    use punkte_server::google::{DesktopClient, is_loopback};
    let verifier = "v".repeat(43);
    let code = |redirect: &str| json!({ "code": { "code": "4/abc", "verifier": verifier, "redirect_uri": redirect } });

    let app = app(db.clone());
    let (s, b) = call(
        &app,
        "POST",
        "/api/v1/auth/google",
        None,
        Some(code("http://127.0.0.1:5123")),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert!(b.to_string().contains("am Computer"), "{b}");

    let mut state = AppState::new(db, SECRET);
    state.google = Arc::new(
        GoogleAuth::with_keys(vec![CLIENT.into()], vec![]).with_desktop(DesktopClient {
            id: "desk".into(),
            secret: "s".into(),
        }),
    );
    let app = routes::router(state, &[]);
    for bad in [
        "https://evil.example/cb",
        "http://127.0.0.1.evil.example:80",
        "http://localhost:5123",
        "http://127.0.0.1:0",
    ] {
        let (s, b) = call(&app, "POST", "/api/v1/auth/google", None, Some(code(bad))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{bad}: {b}");
    }
    let (s, _) = call(
        &app,
        "POST",
        "/api/v1/auth/google",
        None,
        Some(json!({ "device_name": "PC" })),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    assert!(is_loopback("http://127.0.0.1:5123") && is_loopback("http://127.0.0.1:5123/"));
    assert!(!is_loopback("http://127.0.0.1:5123/x") && !is_loopback("http://127.0.0.1:"));
}
