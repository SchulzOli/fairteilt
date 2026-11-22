//! M1: members, child profiles, PIN login on the family tablet, joining without account.

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

async fn register(app: &Router, email: &str, name: &str) -> String {
    let (s, b) = call(app, "POST", "/api/v1/auth/register", None, Some(json!({ "email": email, "password": "sehr-geheim-123", "display_name": name }))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    b["access_token"].as_str().unwrap().to_string()
}

async fn household(app: &Router, token: &str) -> String {
    let (s, b) = call(app, "POST", "/api/v1/households", Some(token), Some(json!({ "name": "Familie Beispiel", "animal": "fox" }))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    b["household_id"].as_str().unwrap().to_string()
}

async fn invite(app: &Router, token: &str, hid: &str, role: &str) -> String {
    let (s, b) = call(app, "POST", &format!("/api/v1/households/{hid}/invites"), Some(token), Some(json!({ "role": role }))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    b["code"].as_str().unwrap().to_string()
}

/// The M1 scenario: Alex creates the household, Sam joins by code, Mia picks the otter
/// and signs in with a PIN on the family tablet.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn m1_family_scenario(db: PgPool) {
    let app = app(db);
    let alex = register(&app, "alex@example.org", "Alex").await;
    let hid = household(&app, &alex).await;

    let sam = register(&app, "sam@example.org", "Sam").await;
    let code = invite(&app, &alex, &hid, "admin").await;
    let (s, _) = call(&app, "POST", "/api/v1/invites/accept", Some(&sam), Some(json!({ "code": code, "animal": "owl", "color": "#d63384" }))).await;
    assert_eq!(s, StatusCode::CREATED);

    // Mia has no e-mail: Alex creates her profile with the otter and a PIN.
    let (s, child) = call(&app, "POST", &format!("/api/v1/households/{hid}/children"), Some(&alex),
        Some(json!({ "display_name": "Mia", "animal": "otter", "color": "#e8660f", "pin": "4711" }))).await;
    assert_eq!(s, StatusCode::CREATED, "{child}");
    let mia_mid = child["membership_id"].as_str().unwrap().to_string();

    let (s, members) = call(&app, "GET", &format!("/api/v1/households/{hid}/members"), Some(&sam), None).await;
    assert_eq!(s, StatusCode::OK);
    let names: Vec<&str> = members.as_array().unwrap().iter().map(|m| m["display_name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Alex", "Sam", "Mia"]);
    let mia = &members[2];
    assert_eq!((mia["role"].as_str(), mia["animal"].as_str(), mia["has_pin"].as_bool(), mia["has_email"].as_bool()),
        (Some("child"), Some("otter"), Some(true), Some(false)));

    // The tablet is signed in as Alex; Mia switches to her profile with the PIN.
    let (s, wrong) = call(&app, "POST", "/api/v1/auth/pin", Some(&alex), Some(json!({ "membership_id": mia_mid, "pin": "0000" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "{wrong}");
    let (s, tokens) = call(&app, "POST", "/api/v1/auth/pin", Some(&alex), Some(json!({ "membership_id": mia_mid, "pin": "4711" }))).await;
    assert_eq!(s, StatusCode::OK, "{tokens}");
    let mia_token = tokens["access_token"].as_str().unwrap();
    let (s, me) = call(&app, "GET", "/api/v1/me", Some(mia_token), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(me["user"]["display_name"], "Mia");
    assert_eq!(me["user"]["email"], Value::Null);
    assert_eq!(me["memberships"][0]["animal"], "otter");
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn pin_needs_household_device_and_locks_after_five_fails(db: PgPool) {
    let app = app(db);
    let alex = register(&app, "alex@example.org", "Alex").await;
    let hid = household(&app, &alex).await;
    let (_, child) = call(&app, "POST", &format!("/api/v1/households/{hid}/children"), Some(&alex),
        Some(json!({ "display_name": "Mia", "pin": "4711" }))).await;
    let mid = child["membership_id"].as_str().unwrap().to_string();

    // A stranger's device cannot try PINs for this household, even with the right PIN.
    let eve = register(&app, "eve@example.org", "Eve").await;
    let (s, _) = call(&app, "POST", "/api/v1/auth/pin", Some(&eve), Some(json!({ "membership_id": mid, "pin": "4711" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // Without any session it is not even considered.
    let (s, _) = call(&app, "POST", "/api/v1/auth/pin", None, Some(json!({ "membership_id": mid, "pin": "4711" }))).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    for _ in 0..5 {
        let (s, _) = call(&app, "POST", "/api/v1/auth/pin", Some(&alex), Some(json!({ "membership_id": mid, "pin": "1234" }))).await;
        assert_eq!(s, StatusCode::BAD_REQUEST);
    }
    let (s, _) = call(&app, "POST", "/api/v1/auth/pin", Some(&alex), Some(json!({ "membership_id": mid, "pin": "4711" }))).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS, "locked even with the right PIN");

    // Setting a new PIN (admin) lifts the lock.
    let (s, _) = call(&app, "PUT", &format!("/api/v1/households/{hid}/members/{mid}/pin"), Some(&alex), Some(json!({ "pin": "2468" }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = call(&app, "POST", "/api/v1/auth/pin", Some(&alex), Some(json!({ "membership_id": mid, "pin": "2468" }))).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = call(&app, "PUT", &format!("/api/v1/households/{hid}/members/{mid}/pin"), Some(&alex), Some(json!({ "pin": "12ab" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "PINs are digits only");
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn child_joins_on_own_device_without_account(db: PgPool) {
    let app = app(db);
    let alex = register(&app, "alex@example.org", "Alex").await;
    let hid = household(&app, &alex).await;

    let adult_code = invite(&app, &alex, &hid, "member").await;
    let (s, _) = call(&app, "POST", "/api/v1/invites/join", None, Some(json!({ "code": adult_code, "display_name": "Sam" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "adult invites need an account with e-mail");

    let code = invite(&app, &alex, &hid, "child").await;
    let (s, joined) = call(&app, "POST", "/api/v1/invites/join", None,
        Some(json!({ "code": code, "display_name": "Mia", "animal": "otter", "color": "#e8660f", "device_name": "Mias Handy" }))).await;
    assert_eq!(s, StatusCode::CREATED, "{joined}");
    assert_eq!(joined["household_id"].as_str().unwrap(), hid);
    let (s, me) = call(&app, "GET", "/api/v1/me", Some(joined["access_token"].as_str().unwrap()), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(me["memberships"][0]["role"], "child");

    let (s, _) = call(&app, "POST", "/api/v1/invites/join", None, Some(json!({ "code": code, "display_name": "Nochmal" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "codes are single use");
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn member_updates_and_permissions(db: PgPool) {
    let app = app(db);
    let alex = register(&app, "alex@example.org", "Alex").await;
    let hid = household(&app, &alex).await;
    let sam = register(&app, "sam@example.org", "Sam").await;
    let code = invite(&app, &alex, &hid, "member").await;
    let (_, acc) = call(&app, "POST", "/api/v1/invites/accept", Some(&sam), Some(json!({ "code": code }))).await;
    let sam_mid = acc["membership_id"].as_str().unwrap().to_string();
    let (_, me) = call(&app, "GET", "/api/v1/me", Some(&alex), None).await;
    let alex_mid = me["memberships"][0]["membership_id"].as_str().unwrap().to_string();

    // Sam changes own animal; cannot change own weight or Alex.
    let (s, _) = call(&app, "PATCH", &format!("/api/v1/households/{hid}/members/{sam_mid}"), Some(&sam), Some(json!({ "animal": "whale" }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = call(&app, "PATCH", &format!("/api/v1/households/{hid}/members/{sam_mid}"), Some(&sam), Some(json!({ "weight": 2.0 }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(&app, "PATCH", &format!("/api/v1/households/{hid}/members/{alex_mid}"), Some(&sam), Some(json!({ "animal": "cat" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Admin sets weight and role; the last admin cannot step down.
    let (s, _) = call(&app, "PATCH", &format!("/api/v1/households/{hid}/members/{sam_mid}"), Some(&alex), Some(json!({ "weight": 0.8 }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = call(&app, "PATCH", &format!("/api/v1/households/{hid}/members/{alex_mid}"), Some(&alex), Some(json!({ "role": "member" }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = call(&app, "PATCH", &format!("/api/v1/households/{hid}/members/{sam_mid}"), Some(&alex), Some(json!({ "animal": "unicorn" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let (_, members) = call(&app, "GET", &format!("/api/v1/households/{hid}/members"), Some(&alex), None).await;
    assert_eq!(members[1]["animal"], "whale");
    assert_eq!(members[1]["weight"], 0.8);
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn login_is_rate_limited(db: PgPool) {
    let app = app(db);
    register(&app, "alex@example.org", "Alex").await;
    let mut last = StatusCode::OK;
    for _ in 0..11 {
        let (s, _) = call(&app, "POST", "/api/v1/auth/login", None, Some(json!({ "email": "alex@example.org", "password": "falsch-falsch" }))).await;
        last = s;
    }
    assert_eq!(last, StatusCode::TOO_MANY_REQUESTS, "11th attempt within 15 minutes is refused");
}
