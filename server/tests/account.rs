//! M7: idempotent writes (offline queue), data export, deleting account, member and household.

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use punkte_server::{AppState, routes};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const SECRET: &[u8] = b"test-secret-test-secret-test-secret!!";
const PW: &str = "sehr-geheim-123";

async fn send(app: &Router, method: &str, path: &str, token: Option<&str>, body: Option<Value>, key: Option<&str>) -> (StatusCode, HeaderMap, Value) {
    let mut req = Request::builder().method(method).uri(path);
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    if let Some(k) = key {
        req = req.header("idempotency-key", k);
    }
    let req = match body {
        Some(b) => req.header("content-type", "application/json").body(Body::from(b.to_string())),
        None => req.body(Body::empty()),
    }
    .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let (status, headers) = (res.status(), res.headers().clone());
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or(Value::Null) };
    (status, headers, json)
}

async fn call(app: &Router, method: &str, path: &str, token: Option<&str>, body: Option<Value>) -> (StatusCode, Value) {
    let (s, _, b) = send(app, method, path, token, body, None).await;
    (s, b)
}

async fn register(app: &Router, email: &str, name: &str) -> String {
    let (s, b) = call(app, "POST", "/api/v1/auth/register", None, Some(json!({ "email": email, "password": PW, "display_name": name }))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    b["access_token"].as_str().unwrap().to_string()
}

struct Family {
    alex: String,
    sam: String,
    hid: String,
    sam_mid: String,
    mia_mid: String,
}

/// Alex (admin), Sam (member), Mia (child with PIN).
async fn family(app: &Router) -> Family {
    let alex = register(app, "alex@example.org", "Alex").await;
    let (_, b) = call(app, "POST", "/api/v1/households", Some(&alex), Some(json!({ "name": "Familie Beispiel", "animal": "fox" }))).await;
    let hid = b["household_id"].as_str().unwrap().to_string();
    let sam = register(app, "sam@example.org", "Sam").await;
    let (_, inv) = call(app, "POST", &format!("/api/v1/households/{hid}/invites"), Some(&alex), Some(json!({ "role": "member" }))).await;
    call(app, "POST", "/api/v1/invites/accept", Some(&sam), Some(json!({ "code": inv["code"], "animal": "owl", "color": "#d63384" }))).await;
    let (_, child) = call(app, "POST", &format!("/api/v1/households/{hid}/children"), Some(&alex),
        Some(json!({ "display_name": "Mia", "animal": "otter", "color": "#e8660f", "pin": "4711" }))).await;
    let (_, members) = call(app, "GET", &format!("/api/v1/households/{hid}/members"), Some(&alex), None).await;
    Family { alex, sam, hid, sam_mid: members[1]["membership_id"].as_str().unwrap().to_string(), mia_mid: child["membership_id"].as_str().unwrap().to_string() }
}

async fn once(app: &Router, f: &Family, title: &str, member: Option<&str>) -> String {
    let assignment = match member {
        Some(m) => json!({ "type": "fixed", "member": m }),
        None => json!({ "type": "open" }),
    };
    let (s, b) = call(app, "POST", &format!("/api/v1/households/{}/templates", f.hid), Some(&f.alex), Some(json!({
        "title": title, "schedule": { "type": "once" }, "assignment": assignment, "steps": [], "step_order": "free"
    }))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    b["task"]["id"].as_str().unwrap().to_string()
}

/// The offline queue sends "done" again after a lost answer: the same key gets the same
/// answer, the XP are booked once.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn idempotent_writes(db: PgPool) {
    let app = routes::router(AppState::new(db.clone(), SECRET), &[]);
    let f = family(&app).await;
    let tid = once(&app, &f, "Einkaufen", None).await;
    let path = format!("/api/v1/tasks/{tid}/action");
    let key = punkte_server::idempotency::new_key();

    let (s, h, first) = send(&app, "POST", &path, Some(&f.alex), Some(json!({ "action": "done" })), Some(&key)).await;
    assert_eq!((s, first["xp_awarded"].as_i64()), (StatusCode::OK, Some(10)));
    assert!(h.get("idempotent-replay").is_none());
    let (s, h, again) = send(&app, "POST", &path, Some(&f.alex), Some(json!({ "action": "done" })), Some(&key)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(h.get("idempotent-replay").unwrap(), "true");
    assert_eq!(again, first);
    let rewards: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_entries WHERE kind = 'task_reward'").fetch_one(&db).await.unwrap();
    assert_eq!(rewards, 1, "booked once");

    // Without the key it is a normal second request (the task is done already).
    let (s, _) = call(&app, "POST", &path, Some(&f.alex), Some(json!({ "action": "done" }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    // A key belongs to one request: reused elsewhere it is refused; another user has their own keys.
    let (s, _, _) = send(&app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(&f.alex), Some(json!({ "action": "claim" })), Some(&key)).await;
    assert_eq!(s, StatusCode::OK, "same path and method replay the stored answer");
    let other = once(&app, &f, "Kochen", None).await;
    let (s, _, _) = send(&app, "POST", &format!("/api/v1/tasks/{other}/action"), Some(&f.alex), Some(json!({ "action": "done" })), Some(&key)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    let (s, _, b) = send(&app, "POST", &format!("/api/v1/tasks/{other}/action"), Some(&f.sam), Some(json!({ "action": "claim" })), Some(&key)).await;
    assert_eq!((s, b["task"]["status"].as_str()), (StatusCode::OK, Some("claimed")));
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn export_my_data_and_the_household(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let f = family(&app).await;
    let tid = once(&app, &f, "Rasen mähen", None).await;
    call(&app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(&f.sam), Some(json!({ "action": "done" }))).await;

    let (s, h, me) = send(&app, "GET", "/api/v1/me/export", Some(&f.sam), None, None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(h.get("content-disposition").unwrap().to_str().unwrap().starts_with("attachment; filename=\"fairteilt-meine-daten-"));
    assert_eq!((me["format"].as_str(), me["account"]["email"].as_str()), (Some("fairteilt-export-1"), Some("sam@example.org")));
    let household = &me["households"][0];
    assert_eq!(household["household"]["name"], "Familie Beispiel");
    assert_eq!(household["ledger"][0]["contribution_delta"], 10);
    assert_eq!(household["tasks_done"][0]["title"], "Rasen mähen");
    assert!(me.to_string().find("password").is_none() && me.to_string().find("pin_hash").is_none(), "no secrets");

    let path = format!("/api/v1/households/{}/export", f.hid);
    let (s, _) = call(&app, "GET", &path, Some(&f.sam), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "the whole household only for admins");
    let (s, all) = call(&app, "GET", &path, Some(&f.alex), None).await;
    assert_eq!(s, StatusCode::OK);
    let names: Vec<&str> = all["members"].as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Alex", "Sam", "Mia"]);
    assert_eq!(all["rooms"].as_array().unwrap().len(), 5);
    assert_eq!(all["ledger"].as_array().unwrap().len(), 1);
    assert!(all.to_string().find("pin_hash").is_none() && all.to_string().find("password_hash").is_none());
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn delete_account_and_member(db: PgPool) {
    let app = routes::router(AppState::new(db.clone(), SECRET), &[]);
    let f = family(&app).await;
    let tid = once(&app, &f, "Müll", None).await;
    call(&app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(&f.sam), Some(json!({ "action": "done" }))).await;
    let mine = once(&app, &f, "Wäsche", Some(&f.sam_mid)).await;

    // Sam deletes his account: password needed; history stays, anonymous.
    let (s, _) = call(&app, "DELETE", "/api/v1/me", Some(&f.sam), Some(json!({ "password": "falsch" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "DELETE", "/api/v1/me", Some(&f.sam), Some(json!({ "password": PW }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = call(&app, "GET", "/api/v1/me", Some(&f.sam), None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED, "his sessions are gone");
    let (s, _) = call(&app, "POST", "/api/v1/auth/login", None, Some(json!({ "email": "sam@example.org", "password": PW }))).await;
    assert_ne!(s, StatusCode::OK);
    let (_, members) = call(&app, "GET", &format!("/api/v1/households/{}/members", f.hid), Some(&f.alex), None).await;
    assert_eq!(members.as_array().unwrap().len(), 2);
    let ledger: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ledger_entries WHERE member_id = $1::uuid").bind(&f.sam_mid).fetch_one(&db).await.unwrap();
    assert_eq!(ledger, 1, "the household's history keeps his XP");
    let name: String = sqlx::query_scalar("SELECT u.display_name FROM users u JOIN memberships m ON m.user_id = u.id WHERE m.id = $1::uuid")
        .bind(&f.sam_mid)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(name, "Gelöschtes Konto");
    let (_, t) = call(&app, "GET", &format!("/api/v1/tasks/{mine}"), Some(&f.alex), None).await;
    assert_eq!((t["status"].as_str(), t["assignee_id"].as_str()), (Some("open"), None), "his open task is free again");

    // Alex is the only admin while Mia stays: he must hand over or delete the household.
    let (s, b) = call(&app, "DELETE", "/api/v1/me", Some(&f.alex), Some(json!({ "password": PW }))).await;
    assert_eq!(s, StatusCode::CONFLICT, "{b}");

    // Admins remove members; a child profile is anonymized and its PIN stops working.
    let (s, _) = call(&app, "DELETE", &format!("/api/v1/households/{}/members/{}", f.hid, f.mia_mid), Some(&f.alex), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = call(&app, "POST", "/api/v1/auth/pin", Some(&f.alex), Some(json!({ "membership_id": f.mia_mid, "pin": "4711" }))).await;
    assert_ne!(s, StatusCode::OK);

    // Now Alex is alone: deleting his account deletes the household too.
    let (s, _) = call(&app, "DELETE", "/api/v1/me", Some(&f.alex), Some(json!({ "password": PW }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM households").fetch_one(&db).await.unwrap();
    assert_eq!(left, 0);
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn delete_household(db: PgPool) {
    let app = routes::router(AppState::new(db.clone(), SECRET), &[]);
    let f = family(&app).await;
    let tid = once(&app, &f, "Fenster", None).await;
    call(&app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(&f.alex), Some(json!({ "action": "done" }))).await;
    let path = format!("/api/v1/households/{}/delete", f.hid);
    let (s, _) = call(&app, "POST", &path, Some(&f.sam), Some(json!({ "confirm_name": "Familie Beispiel" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(&app, "POST", &path, Some(&f.alex), Some(json!({ "confirm_name": "Familie" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, b) = call(&app, "POST", &path, Some(&f.alex), Some(json!({ "confirm_name": "Familie Beispiel" }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{b}");
    for table in ["households", "memberships", "tasks", "ledger_entries", "periods", "rooms"] {
        let n: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT COUNT(*) FROM {table}"))).fetch_one(&db).await.unwrap();
        assert_eq!(n, 0, "{table}");
    }
    // Mia's profile (no login of her own) is gone, the adults keep their accounts.
    let users: Vec<String> = sqlx::query_scalar("SELECT display_name FROM users ORDER BY display_name").fetch_all(&db).await.unwrap();
    assert_eq!(users, ["Alex", "Sam"]);
    let (_, me) = call(&app, "GET", "/api/v1/me", Some(&f.alex), None).await;
    assert!(me["memberships"].as_array().unwrap().is_empty());
}
