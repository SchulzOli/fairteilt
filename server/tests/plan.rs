//! M6: calendar with coming rounds, scheduling and assigning, board positions, saved views.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{Duration, Utc};
use http_body_util::BodyExt;
use punkte_server::{AppState, routes};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const SECRET: &[u8] = b"test-secret-test-secret-test-secret!!";

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

struct Family {
    alex: String,
    sam: String,
    mia: String,
    hid: String,
    alex_mid: String,
    sam_mid: String,
    mia_mid: String,
}

async fn family(app: &Router) -> Family {
    let alex = register(app, "alex@example.org", "Alex").await;
    let (_, b) = call(app, "POST", "/api/v1/households", Some(&alex), Some(json!({ "name": "Familie Beispiel", "animal": "fox" }))).await;
    let hid = b["household_id"].as_str().unwrap().to_string();
    let sam = register(app, "sam@example.org", "Sam").await;
    let (_, inv) = call(app, "POST", &format!("/api/v1/households/{hid}/invites"), Some(&alex), Some(json!({ "role": "admin" }))).await;
    call(app, "POST", "/api/v1/invites/accept", Some(&sam), Some(json!({ "code": inv["code"], "animal": "owl", "color": "#d63384" }))).await;
    let (_, child) = call(app, "POST", &format!("/api/v1/households/{hid}/children"), Some(&alex),
        Some(json!({ "display_name": "Mia", "animal": "otter", "color": "#e8660f", "pin": "4711" }))).await;
    let mia_mid = child["membership_id"].as_str().unwrap().to_string();
    let (_, tokens) = call(app, "POST", "/api/v1/auth/pin", Some(&alex), Some(json!({ "membership_id": mia_mid, "pin": "4711" }))).await;
    let mia = tokens["access_token"].as_str().unwrap().to_string();
    let (_, members) = call(app, "GET", &format!("/api/v1/households/{hid}/members"), Some(&alex), None).await;
    let mid = |i: usize| members[i]["membership_id"].as_str().unwrap().to_string();
    Family { alex, sam, mia, hid, alex_mid: mid(0), sam_mid: mid(1), mia_mid }
}

async fn template(app: &Router, f: &Family, body: Value) -> Value {
    let (s, b) = call(app, "POST", &format!("/api/v1/households/{}/templates", f.hid), Some(&f.alex), Some(body)).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    b
}

fn url_time(t: chrono::DateTime<Utc>) -> String {
    t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true).replace('+', "%2B")
}

/// The M6 scenario: the calendar shows this week's round and the coming ones (who is next
/// in the rotation), a task is planned on a time slot and handed to Mia on the board.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn m6_calendar_planning_board(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let f = family(&app).await;
    let bath = template(&app, &f, json!({
        "title": "Bad putzen", "schedule": { "type": "interval", "days": 7 },
        "assignment": { "type": "rotation", "members": [f.alex_mid, f.sam_mid], "next": 0 }, "steps": [], "step_order": "free"
    }))
    .await;
    let bath_task = bath["task"]["id"].as_str().unwrap().to_string();

    let now = Utc::now();
    let cal = format!("/api/v1/households/{}/calendar?from={}&to={}", f.hid, url_time(now - Duration::hours(1)), url_time(now + Duration::days(30)));
    let (s, entries) = call(&app, "GET", &cal, Some(&f.mia), None).await;
    assert_eq!(s, StatusCode::OK, "{entries}");
    let rounds: Vec<&Value> = entries.as_array().unwrap().iter().filter(|e| e["title"] == "Bad putzen").collect();
    assert_eq!(rounds.len(), 4, "this week's round and three coming ones: {rounds:?}");
    assert_eq!((rounds[0]["projected"].as_bool(), rounds[0]["task_id"].as_str()), (Some(false), Some(bath_task.as_str())));
    let who: Vec<&str> = rounds.iter().map(|r| r["assignee_id"].as_str().unwrap()).collect();
    assert_eq!(who, [f.alex_mid.as_str(), f.sam_mid.as_str(), f.alex_mid.as_str(), f.sam_mid.as_str()], "rotation carries on");
    assert!(rounds[1..].iter().all(|r| r["projected"] == true && r["task_id"].is_null()));
    let (s, _) = call(&app, "GET", &format!("/api/v1/households/{}/calendar?from={}&to={}", f.hid, url_time(now), url_time(now + Duration::days(90))), Some(&f.mia), None).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "at most 62 days");

    // Planned for tomorrow 10:00 (half an hour by default); children only plan their own.
    let slot = (now + Duration::days(1)).date_naive().and_hms_opt(9, 0, 0).unwrap().and_utc();
    let (s, _) = call(&app, "PUT", &format!("/api/v1/tasks/{bath_task}/schedule"), Some(&f.mia), Some(json!({ "scheduled_start": slot }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, t) = call(&app, "PUT", &format!("/api/v1/tasks/{bath_task}/schedule"), Some(&f.alex), Some(json!({ "scheduled_start": slot }))).await;
    assert_eq!(s, StatusCode::OK, "{t}");
    assert_eq!(t["scheduled_end"].as_str().unwrap().parse::<chrono::DateTime<Utc>>().unwrap(), slot + Duration::minutes(30));
    let (_, entries) = call(&app, "GET", &cal, Some(&f.mia), None).await;
    assert!(entries.as_array().unwrap().iter().any(|e| e["task_id"] == bath_task.as_str() && !e["scheduled_start"].is_null()));
    let (s, _) = call(&app, "PUT", &format!("/api/v1/tasks/{bath_task}/schedule"), Some(&f.alex), Some(json!({ "scheduled_start": slot, "scheduled_end": slot }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Board: an open task dropped into Mia's zone is hers (and she hears about it).
    let dishes = template(&app, &f, json!({
        "title": "Spülmaschine", "schedule": { "type": "interval", "days": 1 }, "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    }))
    .await;
    let dishes_task = dishes["task"]["id"].as_str().unwrap().to_string();
    let (s, _) = call(&app, "PUT", &format!("/api/v1/tasks/{dishes_task}/assign"), Some(&f.mia), Some(json!({ "assignee_id": f.mia_mid }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "children do not assign");
    let (s, t) = call(&app, "PUT", &format!("/api/v1/tasks/{dishes_task}/assign"), Some(&f.sam), Some(json!({ "assignee_id": f.mia_mid }))).await;
    assert_eq!((s, t["status"].as_str(), t["assignee_id"].as_str()), (StatusCode::OK, Some("claimed"), Some(f.mia_mid.as_str())));
    let (_, inbox) = call(&app, "GET", &format!("/api/v1/households/{}/inbox", f.hid), Some(&f.mia), None).await;
    assert_eq!(inbox[0]["title"], "Neue Aufgabe für dich: „Spülmaschine“");
    // Back to open; then Alex takes it himself.
    let (_, t) = call(&app, "PUT", &format!("/api/v1/tasks/{dishes_task}/assign"), Some(&f.sam), Some(json!({ "assignee_id": null }))).await;
    assert_eq!((t["status"].as_str(), t["assignee_id"].as_str()), (Some("open"), None));
    let (s, r) = call(&app, "POST", &format!("/api/v1/tasks/{dishes_task}/action"), Some(&f.alex), Some(json!({ "action": "claim" }))).await;
    assert_eq!((s, r["task"]["assignee_id"].as_str()), (StatusCode::OK, Some(f.alex_mid.as_str())), "{r}");

    // Card positions; a recurring card keeps its place in the next round.
    let board = format!("/api/v1/households/{}/board", f.hid);
    let (s, _) = call(&app, "PUT", &board, Some(&f.alex), Some(json!({ "task_id": dishes_task, "x": 120.0, "y": 80.0 }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    call(&app, "PUT", &board, Some(&f.alex), Some(json!({ "task_id": dishes_task, "x": 300.0, "y": 40.0 }))).await;
    let (s, _) = call(&app, "PUT", &board, Some(&f.mia), Some(json!({ "task_id": dishes_task, "x": 1.0, "y": 1.0 }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, r) = call(&app, "POST", &format!("/api/v1/tasks/{dishes_task}/action"), Some(&f.alex), Some(json!({ "action": "done" }))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let (_, items) = call(&app, "GET", &board, Some(&f.mia), None).await;
    let items = items.as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!((items[0]["x"].as_f64(), items[0]["template_id"].as_str()), (Some(300.0), dishes["template_id"].as_str()));
    // A finished round is not handed to anyone any more.
    let (s, _) = call(&app, "PUT", &format!("/api/v1/tasks/{dishes_task}/assign"), Some(&f.sam), Some(json!({ "assignee_id": f.mia_mid }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn saved_views(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let f = family(&app).await;
    let path = format!("/api/v1/households/{}/views", f.hid);
    let view = |name: &str, shared: bool| json!({
        "name": name, "shared": shared, "layout": "kanban",
        "filter": { "members": [f.mia_mid], "open": true, "rooms": [], "statuses": ["open", "claimed"], "kinds": [], "bounty_only": false, "due_within_days": 7 },
        "sort": { "by": "due", "group": "member" }
    });
    let (s, mine) = call(&app, "POST", &path, Some(&f.alex), Some(view("Nur ich", false))).await;
    assert_eq!(s, StatusCode::CREATED, "{mine}");
    let (_, shared) = call(&app, "POST", &path, Some(&f.alex), Some(view("Diese Woche", true))).await;
    let mut bad = view("Kaputt", false);
    bad["layout"] = json!("3d");
    let (s, _) = call(&app, "POST", &path, Some(&f.alex), Some(bad)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Sam sees only the shared one; it stays Alex's to edit, admins may remove it.
    let (_, list) = call(&app, "GET", &path, Some(&f.sam), None).await;
    let names: Vec<&str> = list.as_array().unwrap().iter().map(|v| v["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Diese Woche"]);
    assert_eq!(list[0]["filter"]["due_within_days"], 7);
    let sid = shared["id"].as_str().unwrap();
    let (s, _) = call(&app, "PUT", &format!("/api/v1/views/{sid}"), Some(&f.sam), Some(view("Meins", true))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, v) = call(&app, "PUT", &format!("/api/v1/views/{sid}"), Some(&f.alex), Some(view("Wochenplan", true))).await;
    assert_eq!((s, v["name"].as_str()), (StatusCode::OK, Some("Wochenplan")));
    let (s, _) = call(&app, "DELETE", &format!("/api/v1/views/{}", mine["id"].as_str().unwrap()), Some(&f.sam), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "private views are invisible to others");
    let (s, _) = call(&app, "DELETE", &format!("/api/v1/views/{sid}"), Some(&f.mia), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(&app, "DELETE", &format!("/api/v1/views/{sid}"), Some(&f.sam), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = call(&app, "GET", &path, Some(&f.alex), None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}
