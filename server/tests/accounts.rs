//! M1 API tests against a real Postgres (one fresh database per test).
//! Needs DATABASE_URL pointing at a server where the user may create databases,
//! e.g. `docker compose -f deploy/docker-compose.dev.yml up -d`.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use punkte_server::{AppState, routes};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const SECRET: &[u8] = b"test-secret-test-secret-test-secret!!";

fn app(db: PgPool) -> Router {
    routes::router(AppState::new(db, SECRET), &[])
}

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

async fn register(app: &Router, email: &str, name: &str) -> Value {
    let (s, body) = call(
        app,
        "POST",
        "/api/v1/auth/register",
        None,
        Some(json!({ "email": email, "password": "sehr-geheim-123", "display_name": name, "device_name": "Testhandy" })),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{body}");
    body
}

fn token(v: &Value) -> String {
    v["access_token"].as_str().unwrap().to_string()
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn register_login_and_me(db: PgPool) {
    let app = app(db);
    let reg = register(&app, "Alex@Example.org", "Alex").await;
    assert_eq!(reg["user"]["email"], "alex@example.org", "e-mail is stored lowercased");
    assert!(reg["refresh_token"].as_str().unwrap().len() == 64);

    let (s, me) = call(&app, "GET", "/api/v1/me", Some(&token(&reg)), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(me["user"]["display_name"], "Alex");
    assert_eq!(me["memberships"], json!([]));

    let (s, _) = call(&app, "POST", "/api/v1/auth/login", None, Some(json!({ "email": "alex@example.org", "password": "falsch-falsch" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, unknown) = call(&app, "POST", "/api/v1/auth/login", None, Some(json!({ "email": "niemand@example.org", "password": "egal-egal-egal" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(unknown["error"]["message"], "E-Mail-Adresse oder Passwort stimmt nicht.", "same message as a wrong password");

    let (s, login) = call(&app, "POST", "/api/v1/auth/login", None, Some(json!({ "email": " ALEX@example.org ", "password": "sehr-geheim-123" }))).await;
    assert_eq!(s, StatusCode::OK, "{login}");
    let (s, _) = call(&app, "GET", "/api/v1/me", Some(&token(&login)), None).await;
    assert_eq!(s, StatusCode::OK);

    let (s, _) = call(&app, "GET", "/api/v1/me", None, None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = call(&app, "GET", "/api/v1/me", Some("kein.gueltiges.token"), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn registration_validation(db: PgPool) {
    let app = app(db);
    register(&app, "sam@example.org", "Sam").await;
    let (s, _) = call(&app, "POST", "/api/v1/auth/register", None, Some(json!({ "email": "SAM@example.org", "password": "sehr-geheim-123", "display_name": "Sam 2" }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = call(&app, "POST", "/api/v1/auth/register", None, Some(json!({ "email": "kurz@example.org", "password": "kurz", "display_name": "K" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "POST", "/api/v1/auth/register", None, Some(json!({ "email": "keine-mail", "password": "sehr-geheim-123", "display_name": "K" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn refresh_rotates_and_logout_revokes(db: PgPool) {
    let app = app(db);
    let reg = register(&app, "mia@example.org", "Mia").await;
    let old_refresh = reg["refresh_token"].as_str().unwrap().to_string();

    let (s, fresh) = call(&app, "POST", "/api/v1/auth/refresh", None, Some(json!({ "refresh_token": old_refresh }))).await;
    assert_eq!(s, StatusCode::OK, "{fresh}");
    assert_ne!(fresh["refresh_token"], reg["refresh_token"]);

    let (s, _) = call(&app, "POST", "/api/v1/auth/refresh", None, Some(json!({ "refresh_token": old_refresh }))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "a used refresh token must not work again");

    let access = token(&fresh);
    let (s, _) = call(&app, "POST", "/api/v1/auth/logout", Some(&access), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = call(&app, "GET", "/api/v1/me", Some(&access), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "access token of a revoked session is rejected");
    let (s, _) = call(&app, "POST", "/api/v1/auth/refresh", None, Some(json!({ "refresh_token": fresh["refresh_token"] }))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn household_invite_flow(db: PgPool) {
    let app = app(db);
    let alex = token(&register(&app, "alex@example.org", "Alex").await);
    let mia = token(&register(&app, "mia@example.org", "Mia").await);
    let sam = token(&register(&app, "sam@example.org", "Sam").await);

    let (s, hh) = call(&app, "POST", "/api/v1/households", Some(&alex), Some(json!({ "name": "Familie Beispiel", "animal": "fox", "color": "#2563EB" }))).await;
    assert_eq!(s, StatusCode::CREATED, "{hh}");
    let hid = hh["household_id"].as_str().unwrap().to_string();

    // Only members can invite, and only admins.
    let (s, _) = call(&app, "POST", &format!("/api/v1/households/{hid}/invites"), Some(&sam), Some(json!({ "role": "member" }))).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "non-members do not even learn the household exists");

    let (s, inv) = call(&app, "POST", &format!("/api/v1/households/{hid}/invites"), Some(&alex), Some(json!({ "role": "child" }))).await;
    assert_eq!(s, StatusCode::CREATED, "{inv}");
    let code = inv["code"].as_str().unwrap().to_string();
    assert_eq!(code.len(), 9);

    // Mia types it sloppily: lowercase, no dash.
    let sloppy = code.replace('-', " ").to_lowercase();
    let (s, acc) = call(&app, "POST", "/api/v1/invites/accept", Some(&mia), Some(json!({ "code": sloppy, "animal": "otter", "color": "#e8660f" }))).await;
    assert_eq!(s, StatusCode::CREATED, "{acc}");
    assert_eq!(acc["role"], "child");

    let (s, me) = call(&app, "GET", "/api/v1/me", Some(&mia), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(me["memberships"][0]["household_name"], "Familie Beispiel");
    assert_eq!(me["memberships"][0]["animal"], "otter");
    assert_eq!(me["memberships"][0]["weight"], 0.3);

    // Used codes are gone; children cannot invite.
    let (s, _) = call(&app, "POST", "/api/v1/invites/accept", Some(&sam), Some(json!({ "code": code }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "POST", &format!("/api/v1/households/{hid}/invites"), Some(&mia), Some(json!({ "role": "member" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Household got the default rules (monthly, 20 % tolerance).
    let (s, me) = call(&app, "GET", "/api/v1/me", Some(&alex), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(me["memberships"][0]["role"], "admin");
    assert_eq!(me["memberships"][0]["color"], "#2563eb");
}
