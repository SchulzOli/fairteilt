//! M8: shopping lists and "auf Abruf" tasks.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{DateTime, Utc};
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

fn named<'a>(tasks: &'a Value, title: &str) -> Vec<&'a Value> {
    tasks.as_array().unwrap().iter().filter(|t| t["title"] == title).collect()
}

async fn xp_of(app: &Router, f: &Family, mid: &str) -> i64 {
    let (_, accounts) = call(app, "GET", &format!("/api/v1/households/{}/accounts", f.hid), Some(&f.alex), None).await;
    accounts.as_array().unwrap().iter().find(|a| a["membership_id"] == mid).unwrap()["xp_total"].as_i64().unwrap()
}

/// The M8 scenario: Mia writes "Milch" on the list and "Einkaufen" appears. Alex shops, gets
/// the XP, and what was not bought stays for the next round. Nobody set anything up.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn m8_shopping_list_calls_the_task(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let f = family(&app).await;
    let shop = format!("/api/v1/households/{}/shopping", f.hid);
    let tasks = format!("/api/v1/households/{}/tasks", f.hid);

    // Every household starts with "Supermarkt"; its task rests while the list is empty.
    let (s, overview) = call(&app, "GET", &shop, Some(&f.mia), None).await;
    assert_eq!(s, StatusCode::OK, "{overview}");
    assert_eq!(overview["lists"].as_array().unwrap().len(), 1);
    let list = overview["lists"][0].clone();
    assert_eq!((list["name"].as_str(), list["task_id"].is_null()), (Some("Supermarkt"), true));
    let lid = list["id"].as_str().unwrap().to_string();
    let (_, t) = call(&app, "GET", &tasks, Some(&f.mia), None).await;
    assert!(named(&t, "Einkaufen").is_empty());

    // Mia writes something down: no XP, but "Einkaufen" appears for everyone.
    let items = format!("/api/v1/lists/{lid}/items");
    let (s, milk) = call(&app, "POST", &items, Some(&f.mia), Some(json!({ "name": "  milch ", "note": "2 Liter" }))).await;
    assert_eq!((s, milk["name"].as_str(), milk["added_by"].as_str()), (StatusCode::CREATED, Some("Milch"), Some(f.mia_mid.as_str())), "{milk}");
    let (_, t) = call(&app, "GET", &tasks, Some(&f.sam), None).await;
    let round = named(&t, "Einkaufen");
    assert_eq!(round.len(), 1);
    let first = round[0]["id"].as_str().unwrap().to_string();
    assert_eq!((round[0]["on_call"].as_bool(), round[0]["list_id"].as_str(), round[0]["status"].as_str()), (Some(true), Some(lid.as_str()), Some("open")));
    let due: DateTime<Utc> = round[0]["due_at"].as_str().unwrap().parse().unwrap();
    assert!((due - Utc::now()).num_hours() >= 47, "due two days after the trigger");
    assert_eq!(xp_of(&app, &f, &f.mia_mid).await, 0, "writing things down earns nothing");

    // The same item again becomes one line; more items do not open a second round.
    let (s, merged) = call(&app, "POST", &items, Some(&f.sam), Some(json!({ "name": "MILCH", "note": "laktosefrei" }))).await;
    assert_eq!((s, merged["id"].clone(), merged["note"].as_str()), (StatusCode::OK, milk["id"].clone(), Some("2 Liter · laktosefrei")));
    let (_, paste) = call(&app, "POST", &items, Some(&f.sam), Some(json!({ "name": "Zahnpasta", "section": "drugstore" }))).await;
    assert_eq!(paste["section"], "drugstore");
    let (_, t) = call(&app, "GET", &tasks, Some(&f.sam), None).await;
    assert_eq!(named(&t, "Einkaufen").len(), 1);

    // Children only change what they wrote themselves.
    let (s, _) = call(&app, "DELETE", &format!("/api/v1/items/{}", paste["id"].as_str().unwrap()), Some(&f.mia), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // No XP for a trip without shopping.
    let action = format!("/api/v1/tasks/{first}/action");
    let (s, _) = call(&app, "POST", &action, Some(&f.alex), Some(json!({ "action": "done" }))).await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Checking the milk starts the shopping for Alex; it is in the cart now.
    let (s, c) = call(&app, "POST", &format!("/api/v1/items/{}/check", milk["id"].as_str().unwrap()), Some(&f.alex), Some(json!({ "checked": true }))).await;
    assert_eq!((s, c["started_task"].as_str(), c["item"]["checked_by"].as_str()), (StatusCode::OK, Some(first.as_str()), Some(f.alex_mid.as_str())), "{c}");
    let (_, t) = call(&app, "GET", &format!("/api/v1/tasks/{first}"), Some(&f.alex), None).await;
    assert_eq!((t["status"].as_str(), t["assignee_id"].as_str()), (Some("in_progress"), Some(f.alex_mid.as_str())));

    // Mia adds bananas, urgent: Alex hears about it in the shop.
    let (_, bananas) = call(&app, "POST", &items, Some(&f.mia), Some(json!({ "name": "Bananen", "urgent": true }))).await;
    let (_, inbox) = call(&app, "GET", &format!("/api/v1/households/{}/inbox", f.hid), Some(&f.alex), None).await;
    assert_eq!(inbox[0]["title"], "Mia: Bananen noch mit?");
    let (_, t) = call(&app, "GET", &format!("/api/v1/tasks/{first}"), Some(&f.alex), None).await;
    assert_eq!(t["condition_state"], "due", "urgent: due right away");

    // "Einkauf fertig": XP for Alex, the milk leaves the list, the rest calls the next round.
    let (s, done) = call(&app, "POST", &action, Some(&f.alex), Some(json!({ "action": "done" }))).await;
    assert_eq!(s, StatusCode::OK, "{done}");
    assert!(done["xp_awarded"].as_i64().unwrap() > 0);
    let next = done["next_task_id"].as_str().expect("items left: next round").to_string();
    assert_ne!(next, first);
    assert!(xp_of(&app, &f, &f.alex_mid).await > 0);
    let (_, overview) = call(&app, "GET", &shop, Some(&f.sam), None).await;
    let left: Vec<&str> = overview["lists"][0]["items"].as_array().unwrap().iter().map(|i| i["name"].as_str().unwrap()).collect();
    assert_eq!(left, ["Zahnpasta", "Bananen"]);
    assert_eq!(overview["lists"][0]["task_id"].as_str(), Some(next.as_str()));
    assert_eq!((overview["suggestions"][0]["name"].as_str(), overview["suggestions"][0]["times"].as_i64()), (Some("Milch"), Some(1)));

    // Alex takes the next round; Sam brings bananas on the way home: off the list, no XP.
    call(&app, "POST", &format!("/api/v1/tasks/{next}/action"), Some(&f.alex), Some(json!({ "action": "claim" }))).await;
    let (_, c) = call(&app, "POST", &format!("/api/v1/items/{}/check", bananas["id"].as_str().unwrap()), Some(&f.sam), Some(json!({ "checked": true }))).await;
    assert!(c["item"].is_null() && c["started_task"].is_null(), "{c}");
    // Last item gone: the round closes quietly.
    call(&app, "POST", &format!("/api/v1/items/{}/check", paste["id"].as_str().unwrap()), Some(&f.sam), Some(json!({ "checked": true }))).await;
    let (_, t) = call(&app, "GET", &format!("/api/v1/tasks/{next}"), Some(&f.alex), None).await;
    assert_eq!(t["status"], "cancelled");
    let (_, t) = call(&app, "GET", &tasks, Some(&f.sam), None).await;
    assert!(named(&t, "Einkaufen").is_empty());
    assert_eq!(xp_of(&app, &f, &f.sam_mid).await, 0);

    // Wall display: written down on Mia's behalf (adding books nothing, so no PIN).
    let (_, eggs) = call(&app, "POST", &items, Some(&f.alex), Some(json!({ "name": "Eier", "as_member": f.mia_mid }))).await;
    assert_eq!(eggs["added_by"].as_str(), Some(f.mia_mid.as_str()));
    let (s, _) = call(&app, "POST", &items, Some(&f.alex), Some(json!({ "name": "Eier", "as_member": f.hid }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

/// "Altglas wegbringen": an "auf Abruf" task without a list, called by hand. More lists.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn on_call_task_by_hand_and_more_lists(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let f = family(&app).await;
    let templates = format!("/api/v1/households/{}/templates", f.hid);
    let (s, created) = call(&app, "POST", &templates, Some(&f.alex), Some(json!({
        "title": "Altglas wegbringen", "schedule": { "type": "on_call", "due_after_days": 3 }, "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    }))).await;
    assert_eq!((s, created["task"].is_null()), (StatusCode::CREATED, true), "{created}");
    let tid = created["template_id"].as_str().unwrap().to_string();
    let trigger = format!("/api/v1/templates/{tid}/trigger");
    let (s, first) = call(&app, "POST", &trigger, Some(&f.mia), None).await;
    assert_eq!((s, first["created"].as_bool()), (StatusCode::OK, Some(true)), "{first}");
    let (_, again) = call(&app, "POST", &trigger, Some(&f.sam), None).await;
    assert_eq!((again["created"].as_bool(), again["task_id"].clone()), (Some(false), first["task_id"].clone()), "one round at a time");
    // Done: it rests again until the next call.
    let task = first["task_id"].as_str().unwrap();
    let (_, done) = call(&app, "POST", &format!("/api/v1/tasks/{task}/action"), Some(&f.sam), Some(json!({ "action": "done" }))).await;
    assert!(done["next_task_id"].is_null(), "{done}");
    let (_, third) = call(&app, "POST", &trigger, Some(&f.sam), None).await;
    assert_eq!(third["created"].as_bool(), Some(true));
    let (s, _) = call(&app, "POST", &templates, Some(&f.alex), Some(json!({
        "title": "X", "schedule": { "type": "on_call", "due_after_days": 0 }, "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // A fixed rhythm cannot be called.
    let (_, bath) = call(&app, "POST", &templates, Some(&f.alex), Some(json!({
        "title": "Bad", "schedule": { "type": "interval", "days": 7 }, "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    }))).await;
    let (s, _) = call(&app, "POST", &format!("/api/v1/templates/{}/trigger", bath["template_id"].as_str().unwrap()), Some(&f.sam), None).await;
    assert_eq!(s, StatusCode::CONFLICT);

    // A second list with its own task; children do not create lists; deleting removes both.
    let lists = format!("/api/v1/households/{}/lists", f.hid);
    let (s, _) = call(&app, "POST", &lists, Some(&f.mia), Some(json!({ "name": "Drogerie" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, lid) = call(&app, "POST", &lists, Some(&f.sam), Some(json!({ "name": "Drogerie" }))).await;
    assert_eq!(s, StatusCode::CREATED, "{lid}");
    let lid = lid.as_str().unwrap().to_string();
    call(&app, "POST", &format!("/api/v1/lists/{lid}/items"), Some(&f.mia), Some(json!({ "name": "Shampoo" }))).await;
    let (_, t) = call(&app, "GET", &format!("/api/v1/households/{}/tasks", f.hid), Some(&f.sam), None).await;
    assert_eq!(named(&t, "Drogerie einkaufen").len(), 1);
    let (s, _) = call(&app, "DELETE", &format!("/api/v1/lists/{lid}"), Some(&f.alex), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, t) = call(&app, "GET", &format!("/api/v1/households/{}/tasks", f.hid), Some(&f.sam), None).await;
    assert!(named(&t, "Drogerie einkaufen").is_empty());
    let (_, overview) = call(&app, "GET", &format!("/api/v1/households/{}/shopping", f.hid), Some(&f.sam), None).await;
    assert_eq!(overview["lists"].as_array().unwrap().len(), 1);

    // Exports carry the lists; deleting the household takes them along.
    let (s, export) = call(&app, "GET", &format!("/api/v1/households/{}/export", f.hid), Some(&f.alex), None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(export["shopping_lists"].as_array().unwrap().len() >= 2);
    let (s, _) = call(&app, "POST", &format!("/api/v1/households/{}/delete", f.hid), Some(&f.alex), Some(json!({ "confirm_name": "Familie Beispiel" }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let _ = (&f.alex_mid, &f.sam_mid);
}
