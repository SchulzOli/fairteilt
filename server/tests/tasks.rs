//! M2: rooms, templates with rhythm, condition, started tasks, steps, assignment, live updates.

use std::net::SocketAddr;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Duration, Utc};
use futures_util::StreamExt;
use http_body_util::BodyExt;
use punkte_server::{AppState, routes};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio_tungstenite::tungstenite::Message;
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

/// Alex's household with Sam as second admin. Returns (alex, sam, household, alex_mid, sam_mid).
async fn family(app: &Router) -> (String, String, String, String, String) {
    let alex = register(app, "alex@example.org", "Alex").await;
    let (s, b) = call(app, "POST", "/api/v1/households", Some(&alex), Some(json!({ "name": "Familie Beispiel", "animal": "fox" }))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let hid = b["household_id"].as_str().unwrap().to_string();
    let sam = register(app, "sam@example.org", "Sam").await;
    let (_, inv) = call(app, "POST", &format!("/api/v1/households/{hid}/invites"), Some(&alex), Some(json!({ "role": "admin" }))).await;
    let (s, _) = call(app, "POST", "/api/v1/invites/accept", Some(&sam), Some(json!({ "code": inv["code"], "animal": "owl", "color": "#d63384" }))).await;
    assert_eq!(s, StatusCode::CREATED);
    let (_, members) = call(app, "GET", &format!("/api/v1/households/{hid}/members"), Some(&alex), None).await;
    let mid = |i: usize| members[i]["membership_id"].as_str().unwrap().to_string();
    (alex, sam, hid, mid(0), mid(1))
}

fn ts(v: &Value) -> DateTime<Utc> {
    v.as_str().unwrap().parse().unwrap()
}

fn near(a: DateTime<Utc>, b: DateTime<Utc>) -> bool {
    (a - b).num_seconds().abs() < 60
}

/// The M2 scenario: "Bad putzen, alle 7 Tage" turns yellow after 6 days and red after 8;
/// Alex starts it, checks off five steps, and Fairteilt checks in kindly if he gets stuck.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn m2_bathroom_scenario(db: PgPool) {
    let app = routes::router(AppState::new(db.clone(), SECRET), &[]);
    let (alex, sam, hid, alex_mid, sam_mid) = family(&app).await;

    // New households start with rooms.
    let (s, rooms) = call(&app, "GET", &format!("/api/v1/households/{hid}/rooms"), Some(&sam), None).await;
    assert_eq!(s, StatusCode::OK);
    let names: Vec<&str> = rooms.as_array().unwrap().iter().map(|r| r["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["Küche", "Bad", "Wohnzimmer", "Wäsche", "Flur"]);
    let bad = rooms[1]["id"].as_str().unwrap();

    let steps = ["Spiegel", "Waschbecken", "Toilette", "Dusche", "Boden"];
    let (s, created) = call(&app, "POST", &format!("/api/v1/households/{hid}/templates"), Some(&alex), Some(json!({
        "title": "Bad putzen", "room_id": bad, "icon": "bath",
        "points": { "min": 10, "default": 20, "max": 30 },
        "schedule": { "type": "interval", "days": 7 },
        "assignment": { "type": "rotation", "members": [alex_mid, sam_mid], "next": 0 },
        "steps": steps, "step_order": "sequential", "duration_min": 30
    }))).await;
    assert_eq!(s, StatusCode::CREATED, "{created}");
    let task = &created["task"];
    let tid = task["id"].as_str().unwrap().to_string();
    assert_eq!(task["status"], "claimed");
    assert_eq!(task["assignee_id"], alex_mid.as_str(), "rotation starts with Alex");
    assert_eq!(task["condition_state"], "fresh");
    assert_eq!(task["xp"], 20);
    assert_eq!(task["steps"].as_array().unwrap().len(), 5);
    assert!(near(ts(&task["due_at"]), Utc::now() + Duration::days(7)));

    // Day 6 of 7: yellow (due).
    let now = Utc::now();
    sqlx::query("UPDATE tasks SET cycle_start = $2, due_at = $3 WHERE id = $1::uuid")
        .bind(&tid).bind(now - Duration::days(6)).bind(now + Duration::days(1)).execute(&db).await.unwrap();
    let (_, t) = call(&app, "GET", &format!("/api/v1/tasks/{tid}"), Some(&alex), None).await;
    assert_eq!(t["condition_state"], "due", "{t}");
    assert_eq!(t["xp"], 20);

    // Day 8: red (overdue), and one day late it is worth more (+2 XP per day).
    sqlx::query("UPDATE tasks SET cycle_start = $2, due_at = $3 WHERE id = $1::uuid")
        .bind(&tid).bind(now - Duration::days(8)).bind(now - Duration::days(1) - Duration::minutes(5)).execute(&db).await.unwrap();
    let (_, list) = call(&app, "GET", &format!("/api/v1/households/{hid}/tasks"), Some(&sam), None).await;
    assert_eq!(list[0]["condition_state"], "overdue", "{list}");
    assert_eq!(list[0]["xp"], 22);

    // Alex starts it: time counts up from now, the check-in comes after the planned 30 minutes.
    let (s, r) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(&alex), Some(json!({ "action": "start" }))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["task"]["status"], "in_progress");
    let started = ts(&r["task"]["started_at"]);
    assert!(near(started, Utc::now()));
    assert!(near(ts(&r["task"]["remind_at"]), started + Duration::minutes(30)));

    // He gets stuck: "+10 min" moves the gentle check-in.
    let (s, r) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(&alex), Some(json!({ "action": "snooze" }))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert!(near(ts(&r["task"]["remind_at"]), Utc::now() + Duration::minutes(10)));

    // Steps one after another: the floor cannot come before the mirror.
    let step_ids: Vec<String> = r["task"]["steps"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap().to_string()).collect();
    let (s, _) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/steps/{}/check", step_ids[4]), Some(&alex), None).await;
    assert_eq!(s, StatusCode::CONFLICT);
    for sid in &step_ids[..4] {
        let (s, r) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/steps/{sid}/check"), Some(&alex), None).await;
        assert_eq!(s, StatusCode::OK, "{r}");
        assert!(r["xp_awarded"].is_null(), "XP only at the end");
    }
    // Done while a step is still open is refused.
    let (s, _) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(&alex), Some(json!({ "action": "done" }))).await;
    assert_eq!(s, StatusCode::CONFLICT);

    // The fifth step completes the task.
    let (s, r) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/steps/{}/check", step_ids[4]), Some(&alex), None).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["task"]["status"], "completed");
    assert_eq!(r["xp_awarded"], 22);
    assert_eq!(r["task"]["completed_by"], alex_mid.as_str());
    let next = r["next_task_id"].as_str().unwrap();

    // The next round starts fresh now, is due in 7 days and goes to Sam.
    let (_, list) = call(&app, "GET", &format!("/api/v1/households/{hid}/tasks"), Some(&alex), None).await;
    let rounds = list.as_array().unwrap();
    assert_eq!(rounds.len(), 1);
    let n = &rounds[0];
    assert_eq!(n["id"], next);
    assert_eq!((n["condition_state"].as_str(), n["assignee_id"].as_str()), (Some("fresh"), Some(sam_mid.as_str())));
    assert!(near(ts(&n["cycle_start"]), Utc::now()));
    assert!(near(ts(&n["due_at"]), Utc::now() + Duration::days(7)));
    assert!(n["steps"].as_array().unwrap().iter().all(|s| s["done_at"].is_null()));

    // "Least loaded": Alex has 22 XP this month, Sam none, so Sam gets the new task.
    let (s, created) = call(&app, "POST", &format!("/api/v1/households/{hid}/templates"), Some(&alex), Some(json!({
        "title": "Müll rausbringen", "schedule": { "type": "weekly", "weekdays": [1, 4], "every_weeks": 1 },
        "assignment": { "type": "least_loaded", "members": [alex_mid, sam_mid] }, "steps": [], "step_order": "free"
    }))).await;
    assert_eq!(s, StatusCode::CREATED, "{created}");
    assert_eq!(created["task"]["assignee_id"], sam_mid.as_str());
    assert_eq!(created["task"]["xp"], 10, "household default points");

    let (_, templates) = call(&app, "GET", &format!("/api/v1/households/{hid}/templates"), Some(&sam), None).await;
    // Every household also has "Einkaufen" (M8); look the bathroom up by name.
    let bath = templates.as_array().unwrap().iter().find(|t| t["title"] == "Bad putzen").unwrap();
    assert_eq!(bath["steps"].as_array().unwrap().len(), 5);
    assert_eq!(bath["assignment"]["next"], 0, "rotation went Alex, Sam, back to Alex");
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn task_rules_and_rights(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let (alex, sam, hid, _, _) = family(&app).await;
    let (_, child) = call(&app, "POST", &format!("/api/v1/households/{hid}/children"), Some(&alex),
        Some(json!({ "display_name": "Mia", "animal": "otter", "color": "#e8660f", "pin": "4711" }))).await;
    let (_, tokens) = call(&app, "POST", "/api/v1/auth/pin", Some(&alex), Some(json!({ "membership_id": child["membership_id"], "pin": "4711" }))).await;
    let mia = tokens["access_token"].as_str().unwrap();

    let simple = |title: &str| json!({ "title": title, "schedule": { "type": "once" }, "assignment": { "type": "open" }, "steps": [], "step_order": "free" });

    // Children use tasks but do not plan them.
    let (s, _) = call(&app, "POST", &format!("/api/v1/households/{hid}/templates"), Some(mia), Some(simple("Zimmer aufräumen"))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(&app, "POST", &format!("/api/v1/households/{hid}/rooms"), Some(mia), Some(json!({ "name": "Kinderzimmer" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Points outside the household limits (1–100) and nonsense rhythms are refused.
    let mut bad = simple("Zu viel");
    bad["points"] = json!({ "min": 10, "default": 50, "max": 500 });
    let (s, _) = call(&app, "POST", &format!("/api/v1/households/{hid}/templates"), Some(&alex), Some(bad)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let mut bad = simple("Nie");
    bad["schedule"] = json!({ "type": "interval", "days": 0 });
    let (s, _) = call(&app, "POST", &format!("/api/v1/households/{hid}/templates"), Some(&alex), Some(bad)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // An open one-off task: Mia takes and finishes it. No next round.
    let (s, created) = call(&app, "POST", &format!("/api/v1/households/{hid}/templates"), Some(&alex), Some(simple("Blumen gießen"))).await;
    assert_eq!(s, StatusCode::CREATED, "{created}");
    let tid = created["task"]["id"].as_str().unwrap();
    assert_eq!(created["task"]["status"], "open");
    assert!(created["task"]["due_at"].is_null());
    let (s, r) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(mia), Some(json!({ "action": "claim" }))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["task"]["assignee_id"], child["membership_id"]);
    // Snoozing only makes sense while it runs.
    let (s, _) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(mia), Some(json!({ "action": "snooze" }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    // Only admins skip a round.
    let (s, _) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(mia), Some(json!({ "action": "cancel" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, r) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(mia), Some(json!({ "action": "done" }))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    // Children's tasks wait for an adult (household default), then they are credited.
    assert_eq!((r["submitted"].as_bool(), r["task"]["status"].as_str()), (Some(true), Some("submitted")));
    assert!(r["xp_awarded"].is_null());
    let (s, r) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/review"), Some(&alex), Some(json!({ "approve": true }))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["xp_awarded"], 10);
    assert!(r["next_task_id"].is_null());
    let (_, list) = call(&app, "GET", &format!("/api/v1/households/{hid}/tasks"), Some(&sam), None).await;
    assert_eq!(list.as_array().unwrap().len(), 0);

    // Someone from another household sees nothing.
    let eve = register(&app, "eve@example.org", "Eve").await;
    let (s, _) = call(&app, "GET", &format!("/api/v1/tasks/{tid}"), Some(&eve), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    let (s, _) = call(&app, "GET", &format!("/api/v1/households/{hid}/tasks"), Some(&eve), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

/// Connected apps hear about changes right away.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn live_updates(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let (alex, sam, hid, _, sam_mid) = family(&app).await;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let served = app.clone();
    tokio::spawn(async move { axum::serve(listener, served.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap() });

    // A wrong token never gets a socket.
    let refused = tokio_tungstenite::connect_async(format!("ws://{addr}/api/v1/households/{hid}/live?token=nope")).await;
    assert!(refused.is_err());

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/api/v1/households/{hid}/live?token={sam}")).await.unwrap();
    let next = async |ws: &mut tokio_tungstenite::WebSocketStream<_>| -> Value {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next()).await.expect("no event").unwrap().unwrap();
        match msg {
            Message::Text(t) => serde_json::from_str(&t).unwrap(),
            other => panic!("unexpected {other:?}"),
        }
    };
    assert_eq!(next(&mut ws).await["type"], "hello");

    let (s, _) = call(&app, "POST", &format!("/api/v1/households/{hid}/templates"), Some(&alex), Some(json!({
        "title": "Spülmaschine", "schedule": { "type": "interval", "days": 1 }, "assignment": { "type": "open" },
        "steps": ["Einräumen", "Ausräumen"], "step_order": "sequential"
    }))).await;
    assert_eq!(s, StatusCode::CREATED);
    assert_eq!(next(&mut ws).await["type"], "tasks_changed");

    // Sam finishes a task that always needs approval; Alex confirms. Sam's app gets a personal
    // event (for the notice "Alex hat bestätigt"), besides the general reload events.
    let (_, created) = call(&app, "POST", &format!("/api/v1/households/{hid}/templates"), Some(&alex), Some(json!({
        "title": "Fenster putzen", "schedule": { "type": "once" }, "assignment": { "type": "open" },
        "steps": [], "step_order": "free", "approval": "always"
    }))).await;
    let tid = created["task"]["id"].as_str().unwrap().to_string();
    let (_, r) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(&sam), Some(json!({ "action": "done" }))).await;
    assert_eq!(r["submitted"], true, "approval 'always' applies to adults too");
    call(&app, "POST", &format!("/api/v1/tasks/{tid}/review"), Some(&alex), Some(json!({ "approve": true, "note": "Danke!" }))).await;
    let mut personal = Value::Null;
    for _ in 0..8 {
        let e = next(&mut ws).await;
        if e["type"] == "credited" {
            personal = e;
            break;
        }
    }
    assert_eq!((personal["member"].as_str(), personal["xp"].as_i64(), personal["g"].as_i64()), (Some(sam_mid.as_str()), Some(10), Some(10)));
    assert_eq!((personal["task"].as_str(), personal["note"].as_str()), (Some("Fenster putzen"), Some("Danke!")));
}

/// What was set up can be changed: name, steps, rhythm and who is on it follow in the open
/// round (it was not started yet); a task moves to another room; deleting drops the open
/// round. Shopping stays "auf Abruf" and is deleted with its list only.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn edit_move_and_delete_a_setup(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let (alex, sam, hid, alex_mid, sam_mid) = family(&app).await;
    let (s, b) = call(&app, "POST", &format!("/api/v1/households/{hid}/templates"), Some(&alex), Some(json!({
        "title": "Bad putzen", "schedule": { "type": "interval", "days": 7 }, "assignment": { "type": "fixed", "member": alex_mid },
        "steps": ["Waschbecken", "Spiegel"], "step_order": "free"
    }))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let tid = b["template_id"].as_str().unwrap().to_string();
    let task = b["task"]["id"].as_str().unwrap().to_string();

    // Edit: new name, a third step, every 3 days, Sam instead of Alex.
    let (s, e) = call(&app, "PATCH", &format!("/api/v1/templates/{tid}"), Some(&sam), Some(json!({
        "title": "Bad gründlich", "schedule": { "type": "interval", "days": 3 }, "assignment": { "type": "fixed", "member": sam_mid },
        "steps": ["Waschbecken", "Spiegel", "Boden"], "step_order": "sequential"
    }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{e}");
    let (_, t) = call(&app, "GET", &format!("/api/v1/tasks/{task}"), Some(&alex), None).await;
    assert_eq!((t["title"].as_str(), t["assignee_id"].as_str(), t["steps"].as_array().unwrap().len(), t["step_order"].as_str()),
        (Some("Bad gründlich"), Some(sam_mid.as_str()), 3, Some("sequential")));
    let days = (ts(&t["due_at"]) - Utc::now()).num_hours();
    assert!((60..=96).contains(&days), "due in about three days, got {days} h");
    let (_, list) = call(&app, "GET", &format!("/api/v1/households/{hid}/templates"), Some(&alex), None).await;
    let tpl = list.as_array().unwrap().iter().find(|x| x["id"] == tid.as_str()).unwrap().clone();
    assert_eq!((tpl["title"].as_str(), tpl["steps"].as_array().unwrap().len()), (Some("Bad gründlich"), 3));

    // Move to a room: the task and its setup.
    let (_, room) = call(&app, "POST", &format!("/api/v1/households/{hid}/rooms"), Some(&alex), Some(json!({ "name": "Bad" }))).await;
    let rid = room["id"].as_str().unwrap().to_string();
    let (s, _) = call(&app, "PUT", &format!("/api/v1/tasks/{task}/room"), Some(&alex), Some(json!({ "room_id": rid }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, t) = call(&app, "GET", &format!("/api/v1/tasks/{task}"), Some(&alex), None).await;
    assert_eq!(t["room_id"].as_str(), Some(rid.as_str()));
    let (_, list) = call(&app, "GET", &format!("/api/v1/households/{hid}/templates"), Some(&alex), None).await;
    assert_eq!(list.as_array().unwrap().iter().find(|x| x["id"] == tid.as_str()).unwrap()["room_id"].as_str(), Some(rid.as_str()));

    // Delete: gone from the templates and from the open tasks.
    let (s, _) = call(&app, "DELETE", &format!("/api/v1/templates/{tid}"), Some(&alex), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = call(&app, "GET", &format!("/api/v1/households/{hid}/templates"), Some(&alex), None).await;
    assert!(!list.as_array().unwrap().iter().any(|x| x["id"] == tid.as_str()));
    let (_, open) = call(&app, "GET", &format!("/api/v1/households/{hid}/tasks"), Some(&alex), None).await;
    assert!(!open.as_array().unwrap().iter().any(|x| x["id"] == task.as_str()));

    // Shopping: stays "auf Abruf", is not deleted on its own.
    let einkaufen = list.as_array().unwrap().iter().find(|x| x["title"] == "Einkaufen").unwrap()["id"].as_str().unwrap().to_string();
    let (s, _) = call(&app, "PATCH", &format!("/api/v1/templates/{einkaufen}"), Some(&alex), Some(json!({
        "title": "Einkaufen", "schedule": { "type": "interval", "days": 7 }, "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "DELETE", &format!("/api/v1/templates/{einkaufen}"), Some(&alex), None).await;
    assert_eq!(s, StatusCode::CONFLICT);
}
