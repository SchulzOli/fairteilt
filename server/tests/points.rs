//! M3: ledger, approval with adjusted XP and a message, streak, bounties, transfers.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
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

/// Alex (admin) and Sam (admin), Mia (child, signed in by PIN on the tablet).
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

async fn template(app: &Router, token: &str, hid: &str, body: Value) -> String {
    let (s, b) = call(app, "POST", &format!("/api/v1/households/{hid}/templates"), Some(token), Some(body)).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    b["task"]["id"].as_str().unwrap().to_string()
}

async fn account(app: &Router, token: &str, hid: &str, mid: &str) -> Value {
    let (s, list) = call(app, "GET", &format!("/api/v1/households/{hid}/accounts"), Some(token), None).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    list.as_array().unwrap().iter().find(|a| a["membership_id"] == mid).unwrap().clone()
}

async fn act(app: &Router, token: &str, tid: &str, action: &str) -> (StatusCode, Value) {
    call(app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(token), Some(json!({ "action": action }))).await
}

/// The M3 scenario: Mia finishes, a parent confirms with adjusted XP and a message,
/// and her streak counts up.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn m3_approval_scenario(db: PgPool) {
    let app = routes::router(AppState::new(db.clone(), SECRET), &[]);
    let f = family(&app).await;
    let tid = template(&app, &f.alex, &f.hid, json!({
        "title": "Zimmer aufräumen", "points": { "min": 10, "default": 15, "max": 25 },
        "schedule": { "type": "interval", "days": 1 }, "assignment": { "type": "fixed", "member": f.mia_mid },
        "steps": [], "step_order": "free"
    })).await;

    // Mia is done: the task waits for a parent, nothing is credited yet.
    let (s, r) = act(&app, &f.mia, &tid, "done").await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!((r["submitted"].as_bool(), r["task"]["status"].as_str(), r["task"]["xp"].as_i64()), (Some(true), Some("submitted"), Some(15)));
    let a = account(&app, &f.mia, &f.hid, &f.mia_mid).await;
    assert_eq!((a["balance"].as_i64(), a["waiting"].as_i64(), a["streak"]["days"].as_i64()), (Some(0), Some(1), Some(0)));

    // Children do not confirm, and nothing outside the range is accepted.
    let (s, _) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/review"), Some(&f.mia), Some(json!({ "approve": true }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/review"), Some(&f.sam), Some(json!({ "approve": true, "points": 40 }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Sam confirms with 20 instead of 15 XP and a message.
    let (s, r) = call(&app, "POST", &format!("/api/v1/tasks/{tid}/review"), Some(&f.sam),
        Some(json!({ "approve": true, "points": 20, "note": "Super gemacht!" }))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!(r["xp_awarded"], 20);
    assert_eq!((r["task"]["status"].as_str(), r["task"]["approved_by"].as_str()), (Some("completed"), Some(f.sam_mid.as_str())));
    let next = r["next_task_id"].as_str().unwrap().to_string();

    let a = account(&app, &f.mia, &f.hid, &f.mia_mid).await;
    assert_eq!((a["balance"].as_i64(), a["xp_month"].as_i64(), a["waiting"].as_i64()), (Some(20), Some(20), Some(0)));
    assert_eq!(a["streak"], json!({ "days": 1, "today": true }));

    let (_, history) = call(&app, "GET", &format!("/api/v1/households/{}/ledger", f.hid), Some(&f.mia), None).await;
    let e = &history[0];
    assert_eq!((e["kind"].as_str(), e["contribution_delta"].as_i64(), e["balance_delta"].as_i64()), (Some("task_reward"), Some(20), Some(20)));
    assert_eq!((e["note"].as_str(), e["task_title"].as_str()), (Some("Super gemacht!"), Some("Zimmer aufräumen")));
    // Mia may not read Sam's account history; admins may read hers.
    let (s, _) = call(&app, "GET", &format!("/api/v1/households/{}/ledger?member={}", f.hid, f.sam_mid), Some(&f.mia), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, h) = call(&app, "GET", &format!("/api/v1/households/{}/ledger?member={}", f.hid, f.mia_mid), Some(&f.alex), None).await;
    assert_eq!((s, h.as_array().unwrap().len()), (StatusCode::OK, 1));

    // Pretend that was yesterday: today's round makes it a 2-day streak.
    sqlx::query("UPDATE tasks SET completed_at = completed_at - interval '1 day' WHERE id = $1::uuid").bind(&tid).execute(&db).await.unwrap();

    // Round two: first sent back with a message, then confirmed.
    act(&app, &f.mia, &next, "done").await;
    let (s, r) = call(&app, "POST", &format!("/api/v1/tasks/{next}/review"), Some(&f.alex),
        Some(json!({ "approve": false, "note": "Bitte noch das Bett machen" }))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!((r["task"]["status"].as_str(), r["task"]["assignee_id"].as_str()), (Some("claimed"), Some(f.mia_mid.as_str())));
    assert_eq!(r["task"]["review_note"], "Bitte noch das Bett machen");
    assert!(r["xp_awarded"].is_null());
    let (s, r) = act(&app, &f.mia, &next, "done").await;
    assert_eq!((s, r["submitted"].as_bool()), (StatusCode::OK, Some(true)));
    let (s, r) = call(&app, "POST", &format!("/api/v1/tasks/{next}/review"), Some(&f.alex), Some(json!({ "approve": true }))).await;
    assert_eq!((s, r["xp_awarded"].as_i64()), (StatusCode::OK, Some(15)));

    let a = account(&app, &f.mia, &f.hid, &f.mia_mid).await;
    assert_eq!(a["streak"], json!({ "days": 2, "today": true }));
    assert_eq!(a["balance"], 35);
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn bounties_and_catch_up_tasks(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let f = family(&app).await;

    // Adults are credited directly (household rule: only children need approval).
    let car = template(&app, &f.alex, &f.hid, json!({
        "title": "Auto waschen", "points": { "min": 30, "default": 30, "max": 30 },
        "schedule": { "type": "once" }, "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    })).await;
    let (_, r) = act(&app, &f.alex, &car, "done").await;
    assert_eq!(r["xp_awarded"], 30);
    assert_eq!(account(&app, &f.alex, &f.hid, &f.alex_mid).await["balance"], 30);

    // Alex sets 20 G on Mia's task: held right away.
    let room = template(&app, &f.alex, &f.hid, json!({
        "title": "Bad putzen", "schedule": { "type": "once" }, "assignment": { "type": "fixed", "member": f.mia_mid },
        "steps": [], "step_order": "free"
    })).await;
    let (s, t) = call(&app, "POST", &format!("/api/v1/tasks/{room}/bounty"), Some(&f.alex), Some(json!({ "amount": 20 }))).await;
    assert_eq!(s, StatusCode::OK, "{t}");
    assert_eq!(t["bounty"], 20);
    assert_eq!(account(&app, &f.alex, &f.hid, &f.alex_mid).await["balance"], 10);
    // Not more than he has, not on your own task.
    let (s, _) = call(&app, "POST", &format!("/api/v1/tasks/{room}/bounty"), Some(&f.alex), Some(json!({ "amount": 20 }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "POST", &format!("/api/v1/tasks/{room}/bounty"), Some(&f.mia), Some(json!({ "amount": 1 }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Mia does it, Sam confirms: XP plus the bonus.
    act(&app, &f.mia, &room, "done").await;
    let (_, r) = call(&app, "POST", &format!("/api/v1/tasks/{room}/review"), Some(&f.sam), Some(json!({ "approve": true }))).await;
    assert_eq!((r["xp_awarded"].as_i64(), r["bounty_paid"].as_i64()), (Some(10), Some(20)));
    let mia = account(&app, &f.mia, &f.hid, &f.mia_mid).await;
    assert_eq!((mia["balance"].as_i64(), mia["xp_month"].as_i64()), (Some(30), Some(10)), "bonus is G, not XP");

    // A skipped round gives the bonus back.
    let dishes = template(&app, &f.alex, &f.hid, json!({
        "title": "Spülmaschine", "schedule": { "type": "once" }, "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    })).await;
    call(&app, "POST", &format!("/api/v1/tasks/{dishes}/bounty"), Some(&f.alex), Some(json!({ "amount": 10 }))).await;
    assert_eq!(account(&app, &f.alex, &f.hid, &f.alex_mid).await["balance"], 0);
    let (s, _) = act(&app, &f.alex, &dishes, "cancel").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(account(&app, &f.alex, &f.hid, &f.alex_mid).await["balance"], 10);

    // Catch-up tasks (Aufholaufgaben) bring XP but no G.
    let catch_up = template(&app, &f.alex, &f.hid, json!({
        "title": "Keller aufräumen", "kind": "penalty", "schedule": { "type": "once" },
        "assignment": { "type": "fixed", "member": f.sam_mid }, "steps": [], "step_order": "free"
    })).await;
    let (_, r) = act(&app, &f.sam, &catch_up, "done").await;
    assert_eq!(r["xp_awarded"], 10);
    let sam = account(&app, &f.sam, &f.hid, &f.sam_mid).await;
    assert_eq!((sam["balance"].as_i64(), sam["xp_month"].as_i64()), (Some(0), Some(10)));
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn transfers(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let f = family(&app).await;
    // Mia earns 15 G.
    let tid = template(&app, &f.alex, &f.hid, json!({
        "title": "Tisch decken", "points": { "min": 15, "default": 15, "max": 15 }, "schedule": { "type": "once" },
        "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    })).await;
    act(&app, &f.mia, &tid, "done").await;
    call(&app, "POST", &format!("/api/v1/tasks/{tid}/review"), Some(&f.alex), Some(json!({ "approve": true }))).await;

    let path = format!("/api/v1/households/{}/transfers", f.hid);
    // Always with a note, never to yourself, never more than you have.
    let (s, _) = call(&app, "POST", &path, Some(&f.mia), Some(json!({ "to": f.sam_mid, "amount": 5, "note": "  " }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "POST", &path, Some(&f.mia), Some(json!({ "to": f.mia_mid, "amount": 5, "note": "Ich" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "POST", &path, Some(&f.mia), Some(json!({ "to": f.sam_mid, "amount": 50, "note": "Alles" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // A child's transfer waits for an admin.
    let (s, t) = call(&app, "POST", &path, Some(&f.mia), Some(json!({ "to": f.sam_mid, "amount": 5, "note": "Danke fürs Vorlesen" }))).await;
    assert_eq!(s, StatusCode::CREATED, "{t}");
    assert_eq!(t["status"], "pending");
    assert_eq!(account(&app, &f.mia, &f.hid, &f.mia_mid).await["balance"], 15);
    let (_, pending) = call(&app, "GET", &path, Some(&f.alex), None).await;
    assert_eq!(pending.as_array().unwrap().len(), 1, "admins see pending transfers");
    let tid = t["id"].as_str().unwrap();
    let (s, _) = call(&app, "POST", &format!("/api/v1/transfers/{tid}/decide"), Some(&f.mia), Some(json!({ "approve": true }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, t) = call(&app, "POST", &format!("/api/v1/transfers/{tid}/decide"), Some(&f.alex), Some(json!({ "approve": true }))).await;
    assert_eq!((s, t["status"].as_str()), (StatusCode::OK, Some("done")));
    let (s, _) = call(&app, "POST", &format!("/api/v1/transfers/{tid}/decide"), Some(&f.alex), Some(json!({ "approve": true }))).await;
    assert_eq!(s, StatusCode::CONFLICT, "decided only once");

    let mia = account(&app, &f.mia, &f.hid, &f.mia_mid).await;
    let sam = account(&app, &f.sam, &f.hid, &f.sam_mid).await;
    assert_eq!((mia["balance"].as_i64(), sam["balance"].as_i64()), (Some(10), Some(5)));
    assert_eq!((mia["xp_month"].as_i64(), sam["xp_month"].as_i64()), (Some(15), Some(0)), "XP never move");

    // Adults transfer directly; the history shows who and why.
    let (s, t) = call(&app, "POST", &path, Some(&f.sam), Some(json!({ "to": f.alex_mid, "amount": 5, "note": "Für den Müll" }))).await;
    assert_eq!((s, t["status"].as_str()), (StatusCode::CREATED, Some("done")));
    let (_, h) = call(&app, "GET", &format!("/api/v1/households/{}/ledger", f.hid), Some(&f.alex), None).await;
    assert_eq!((h[0]["kind"].as_str(), h[0]["balance_delta"].as_i64()), (Some("transfer_in"), Some(5)));
    assert_eq!((h[0]["note"].as_str(), h[0]["counterpart_id"].as_str()), (Some("Für den Müll"), Some(f.sam_mid.as_str())));
}
