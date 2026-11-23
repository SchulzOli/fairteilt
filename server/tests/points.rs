//! M3: ledger, approval with adjusted XP and a message, streak, bonus XP, corrections.
//! Since the move to XP only, tasks no longer earn Guthaben (G) and there are no transfers.

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

async fn bounty(app: &Router, token: &str, tid: &str, amount: i64) -> (StatusCode, Value) {
    call(app, "POST", &format!("/api/v1/tasks/{tid}/bounty"), Some(token), Some(json!({ "amount": amount }))).await
}

/// Titles of one kind of notification in someone's inbox, newest first.
async fn titles(app: &Router, token: &str, hid: &str, kind: &str) -> Vec<String> {
    let (s, list) = call(app, "GET", &format!("/api/v1/households/{hid}/inbox"), Some(token), None).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    list.as_array().unwrap().iter().filter(|n| n["kind"] == kind).map(|n| n["title"].as_str().unwrap().to_string()).collect()
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
    assert_eq!((a["balance"].as_i64(), a["xp_month"].as_i64(), a["waiting"].as_i64()), (Some(0), Some(20), Some(0)), "XP only, no G");
    assert_eq!(a["streak"], json!({ "days": 1, "today": true }));

    let (_, history) = call(&app, "GET", &format!("/api/v1/households/{}/ledger", f.hid), Some(&f.mia), None).await;
    let e = &history[0];
    assert_eq!((e["kind"].as_str(), e["contribution_delta"].as_i64(), e["balance_delta"].as_i64()), (Some("task_reward"), Some(20), Some(0)));
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
    assert_eq!((a["xp_month"].as_i64(), a["balance"].as_i64()), (Some(35), Some(0)));
    // Confirmed → Mia hears it with her XP (no G any more).
    assert_eq!(titles(&app, &f.mia, &f.hid, "approved").await[0], "„Zimmer aufräumen“ bestätigt · +15 XP");
}

/// Bonus XP is a gift: it costs the giver nothing, everyone on the task shares it together
/// with the task's XP. Each member may give `bonus_xp_per_month` (default 30) per month.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn bounties_and_catch_up_tasks(db: PgPool) {
    let app = routes::router(AppState::new(db.clone(), SECRET), &[]);
    let f = family(&app).await;

    // Adults are credited directly (household rule: only children need approval): XP, no G.
    let car = template(&app, &f.alex, &f.hid, json!({
        "title": "Auto waschen", "points": { "min": 30, "default": 30, "max": 30 },
        "schedule": { "type": "once" }, "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    })).await;
    let (_, r) = act(&app, &f.alex, &car, "done").await;
    assert_eq!(r["xp_awarded"], 30);
    let alex = account(&app, &f.alex, &f.hid, &f.alex_mid).await;
    assert_eq!((alex["xp_month"].as_i64(), alex["balance"].as_i64()), (Some(30), Some(0)));
    // No bonus on finished tasks.
    let (s, _) = bounty(&app, &f.sam, &car, 5).await;
    assert_eq!(s, StatusCode::CONFLICT);

    // Alex puts 10 XP on Mia's task: it costs him nothing, Mia hears about it.
    let room = template(&app, &f.alex, &f.hid, json!({
        "title": "Bad putzen", "points": { "min": 11, "default": 11, "max": 11 }, "schedule": { "type": "once" },
        "assignment": { "type": "fixed", "member": f.mia_mid }, "steps": [], "step_order": "free"
    })).await;
    let (s, t) = bounty(&app, &f.alex, &room, 10).await;
    assert_eq!(s, StatusCode::OK, "{t}");
    assert_eq!(t["bounty"], 10);
    let alex = account(&app, &f.alex, &f.hid, &f.alex_mid).await;
    assert_eq!((alex["xp_month"].as_i64(), alex["balance"].as_i64()), (Some(30), Some(0)), "a gift, nothing held");
    assert_eq!(titles(&app, &f.mia, &f.hid, "bounty").await, ["Bonus auf „Bad putzen“: +10 XP"]);
    let (s, _) = bounty(&app, &f.alex, &room, 0).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Not on a task you are on: Mia's own, and Sam's once he joins it.
    let (s, _) = bounty(&app, &f.mia, &room, 1).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, r) = act(&app, &f.sam, &room, "claim").await;
    assert_eq!((s, &r["task"]["helpers"]), (StatusCode::OK, &json!([f.sam_mid])), "{r}");
    let (s, _) = bounty(&app, &f.sam, &room, 1).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Alex's allowance this month is 30: 20 are left, then it is used up.
    let (s, e) = bounty(&app, &f.alex, &room, 25).await;
    assert_eq!((s, e["error"]["message"].as_str()), (StatusCode::BAD_REQUEST, Some("Diesen Monat kannst du noch 20 XP verschenken.")));
    let (s, t) = bounty(&app, &f.alex, &room, 20).await;
    assert_eq!((s, t["bounty"].as_i64()), (StatusCode::OK, Some(30)), "{t}");
    let (s, e) = bounty(&app, &f.alex, &room, 1).await;
    assert_eq!((s, e["error"]["message"].as_str()), (StatusCode::BAD_REQUEST, Some("Deine XP für Boni sind diesen Monat aufgebraucht.")));
    // Everyone on the task hears about a new bonus.
    assert_eq!(titles(&app, &f.sam, &f.hid, "bounty").await, ["Bonus auf „Bad putzen“: +20 XP"]);

    // Mia does it, Alex confirms: 11 XP + 30 bonus shared by Mia and Sam; Mia, who finished,
    // gets the odd one.
    act(&app, &f.mia, &room, "done").await;
    let (s, r) = call(&app, "POST", &format!("/api/v1/tasks/{room}/review"), Some(&f.alex), Some(json!({ "approve": true }))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!((r["xp_awarded"].as_i64(), r["bounty_paid"].as_i64()), (Some(21), Some(30)));
    let mia = account(&app, &f.mia, &f.hid, &f.mia_mid).await;
    let sam = account(&app, &f.sam, &f.hid, &f.sam_mid).await;
    assert_eq!((mia["xp_month"].as_i64(), mia["balance"].as_i64()), (Some(21), Some(0)));
    assert_eq!((sam["xp_month"].as_i64(), sam["balance"].as_i64()), (Some(20), Some(0)));
    assert_eq!(account(&app, &f.alex, &f.hid, &f.alex_mid).await["xp_month"], 30, "the giver keeps his XP");
    assert_eq!(titles(&app, &f.mia, &f.hid, "approved").await, ["„Bad putzen“ bestätigt · +21 XP"]);
    assert_eq!(titles(&app, &f.sam, &f.hid, "approved").await, ["„Bad putzen“ bestätigt · +20 XP"]);

    // A skipped round: the bonus lapses, nothing is booked, and it no longer counts for the month.
    let dishes = template(&app, &f.alex, &f.hid, json!({
        "title": "Spülmaschine", "schedule": { "type": "once" }, "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    })).await;
    let (s, _) = bounty(&app, &f.sam, &dishes, 30).await;
    assert_eq!(s, StatusCode::OK);
    let (s, r) = act(&app, &f.alex, &dishes, "cancel").await;
    assert_eq!((s, r["task"]["status"].as_str(), r["task"]["bounty"].as_i64()), (StatusCode::OK, Some("cancelled"), Some(0)), "{r}");
    let sam = account(&app, &f.sam, &f.hid, &f.sam_mid).await;
    assert_eq!((sam["xp_month"].as_i64(), sam["balance"].as_i64()), (Some(20), Some(0)));
    let windows = template(&app, &f.alex, &f.hid, json!({
        "title": "Fenster putzen", "schedule": { "type": "once" }, "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    })).await;
    let (s, _) = bounty(&app, &f.sam, &windows, 30).await;
    assert_eq!(s, StatusCode::OK, "Sam's allowance is whole again");

    // A new month, a new allowance.
    let (s, _) = bounty(&app, &f.alex, &windows, 5).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    sqlx::query("UPDATE bounties SET created_at = created_at - interval '40 days' WHERE pledger_id = $1::uuid").bind(&f.alex_mid).execute(&db).await.unwrap();
    let (s, t) = bounty(&app, &f.alex, &windows, 5).await;
    assert_eq!((s, t["bounty"].as_i64()), (StatusCode::OK, Some(35)), "{t}");

    // The household can switch bonuses off (0 per month).
    let rules_path = format!("/api/v1/households/{}/rules", f.hid);
    let (_, mut rules) = call(&app, "GET", &rules_path, Some(&f.alex), None).await;
    assert_eq!(rules["point_policy"]["bonus_xp_per_month"], 30, "default");
    rules["point_policy"]["bonus_xp_per_month"] = json!(0);
    let (s, r) = call(&app, "PUT", &rules_path, Some(&f.alex), Some(rules)).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    let (s, _) = bounty(&app, &f.mia, &windows, 1).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Catch-up tasks (Aufholaufgaben) bring XP like any other task.
    let catch_up = template(&app, &f.alex, &f.hid, json!({
        "title": "Keller aufräumen", "kind": "penalty", "schedule": { "type": "once" },
        "assignment": { "type": "fixed", "member": f.sam_mid }, "steps": [], "step_order": "free"
    })).await;
    let (_, r) = act(&app, &f.sam, &catch_up, "done").await;
    assert_eq!(r["xp_awarded"], 10);
    let sam = account(&app, &f.sam, &f.hid, &f.sam_mid).await;
    assert_eq!((sam["balance"].as_i64(), sam["xp_month"].as_i64()), (Some(0), Some(30)));
}

/// Admins take back a credit with a counter-entry. On a shared task each person has their own
/// credit, and only that one is taken back.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn corrections(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let f = family(&app).await;
    // Alex and Sam mow the lawn together (10 XP, 5 each); Sam finishes.
    let tid = template(&app, &f.alex, &f.hid, json!({
        "title": "Rasen mähen", "schedule": { "type": "once" }, "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    })).await;
    act(&app, &f.alex, &tid, "claim").await;
    act(&app, &f.sam, &tid, "claim").await;
    let (_, r) = act(&app, &f.sam, &tid, "done").await;
    assert_eq!(r["xp_awarded"], 5);

    let (_, h) = call(&app, "GET", &format!("/api/v1/households/{}/ledger?member={}", f.hid, f.sam_mid), Some(&f.alex), None).await;
    let reward = h.as_array().unwrap().iter().find(|e| e["kind"] == "task_reward").unwrap().clone();
    assert_eq!((reward["reversed"].as_bool(), reward["contribution_delta"].as_i64(), reward["balance_delta"].as_i64()), (Some(false), Some(5), Some(0)));
    let path = format!("/api/v1/households/{}/ledger/{}/reverse", f.hid, reward["id"].as_str().unwrap());

    // Only admins, never your own credit, always with a reason.
    let (s, _) = call(&app, "POST", &path, Some(&f.mia), Some(json!({ "note": "Nein" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(&app, "POST", &path, Some(&f.sam), Some(json!({ "note": "Ich" }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = call(&app, "POST", &path, Some(&f.alex), Some(json!({ "note": " " }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    // Sam's 5 XP back (there are no G to take back); Alex keeps his share.
    let (s, r) = call(&app, "POST", &path, Some(&f.alex), Some(json!({ "note": "Rasen war doch nicht gemäht" }))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    assert_eq!((r["xp"].as_i64(), r["g"].as_i64()), (Some(5), Some(0)));
    let sam = account(&app, &f.sam, &f.hid, &f.sam_mid).await;
    assert_eq!((sam["balance"].as_i64(), sam["xp_month"].as_i64()), (Some(0), Some(0)));
    assert_eq!(sam["streak"]["days"], 0, "a taken-back task does not count for the streak");
    assert_eq!(account(&app, &f.alex, &f.hid, &f.alex_mid).await["xp_month"], 5);

    let (_, h) = call(&app, "GET", &format!("/api/v1/households/{}/ledger", f.hid), Some(&f.sam), None).await;
    let fix = &h[0];
    assert_eq!((fix["kind"].as_str(), fix["contribution_delta"].as_i64(), fix["balance_delta"].as_i64()), (Some("manual_adjust"), Some(-5), Some(0)));
    assert_eq!((fix["task_title"].as_str(), fix["note"].as_str()), (Some("Rasen mähen"), Some("Rasen war doch nicht gemäht")));
    assert!(h.as_array().unwrap().iter().any(|e| e["kind"] == "task_reward" && e["reversed"] == true));

    // Only once; only credits for tasks are taken back this way (not the correction itself).
    let (s, _) = call(&app, "POST", &path, Some(&f.alex), Some(json!({ "note": "Nochmal" }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (s, _) = call(&app, "POST", &format!("/api/v1/households/{}/ledger/{}/reverse", f.hid, fix["id"].as_str().unwrap()),
        Some(&f.alex), Some(json!({ "note": "Nein" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}
