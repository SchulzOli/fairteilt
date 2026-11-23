//! M4: monthly close (draft, approval, carry, catch-up tasks), the race against Krümel, rewards, rules.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use punkte_server::{AppState, routes, scheduler};
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

/// A one-off task worth exactly `xp`, done by `who` (adults are credited directly).
async fn earn(app: &Router, f: &Family, who: &str, title: &str, xp: i64) {
    let (s, b) = call(app, "POST", &format!("/api/v1/households/{}/templates", f.hid), Some(&f.alex), Some(json!({
        "title": title, "points": { "min": xp, "default": xp, "max": xp }, "schedule": { "type": "once" },
        "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    }))).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    let tid = b["task"]["id"].as_str().unwrap();
    let (s, r) = call(app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(who), Some(json!({ "action": "done" }))).await;
    assert_eq!(s, StatusCode::OK, "{r}");
    if r["submitted"] == true {
        let (s, r) = call(app, "POST", &format!("/api/v1/tasks/{tid}/review"), Some(&f.alex), Some(json!({ "approve": true }))).await;
        assert_eq!(s, StatusCode::OK, "{r}");
    }
}

async fn period(app: &Router, f: &Family) -> Value {
    let (s, p) = call(app, "GET", &format!("/api/v1/households/{}/period", f.hid), Some(&f.mia), None).await;
    assert_eq!(s, StatusCode::OK, "{p}");
    p
}

async fn set_rules(app: &Router, f: &Family, change: impl FnOnce(&mut Value)) {
    let (_, mut rules) = call(app, "GET", &format!("/api/v1/households/{}/rules", f.hid), Some(&f.alex), None).await;
    change(&mut rules);
    let (s, b) = call(app, "PUT", &format!("/api/v1/households/{}/rules", f.hid), Some(&f.alex), Some(rules)).await;
    assert_eq!(s, StatusCode::OK, "{b}");
}

/// Ends the running period now and runs the due jobs (as the scheduler would).
async fn end_period(db: &PgPool, state: &AppState) {
    sqlx::query("UPDATE periods SET ends_at = now() WHERE status = 'running'").execute(db).await.unwrap();
    sqlx::query("UPDATE jobs SET run_at = now() WHERE kind = 'close_period' AND done_at IS NULL AND payload->>'period_id' IN (SELECT id::text FROM periods WHERE status = 'running')")
        .execute(db)
        .await
        .unwrap();
    assert!(scheduler::run_due(state).await.unwrap() >= 1);
}

async fn balance(app: &Router, f: &Family, mid: &str) -> i64 {
    let (_, list) = call(app, "GET", &format!("/api/v1/households/{}/accounts", f.hid), Some(&f.alex), None).await;
    list.as_array().unwrap().iter().find(|a| a["membership_id"] == mid).unwrap()["balance"].as_i64().unwrap()
}

/// The M4 scenario: the family beats Krümel, the month closes by itself as a draft, an admin
/// approves it: bonus for the one ahead, catch-up tasks for those behind.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn m4_close_and_race(db: PgPool) {
    let state = AppState::new(db.clone(), SECRET);
    let app = routes::router(state.clone(), &[]);
    let f = family(&app).await;

    let p = period(&app, &f).await;
    assert_eq!((p["goal"]["status"].as_str(), p["goal"]["reward"].as_str()), (Some("running"), Some("Familienausflug")));
    assert!(p["goal"]["target"].as_i64().unwrap() >= 20);

    // Rules: goal 30 XP for "Eis essen"; behind = catch-up tasks, ahead = 50 % bonus.
    set_rules(&app, &f, |r| {
        r["goal_policy"]["target"] = json!(30);
        r["goal_policy"]["reward"] = json!("Eis essen");
        r["period_policy"]["deficit_action"] = json!({ "type": "penalty_tasks" });
        r["period_policy"]["surplus_action"] = json!({ "type": "bonus", "percent": 50 });
    })
    .await;
    // The goal can also be set on its own (this period only; later suggested again).
    let (s, g) = call(&app, "PUT", &format!("/api/v1/households/{}/goal", f.hid), Some(&f.mia), Some(json!({ "target": 5, "reward": "Kino", "every_period": false }))).await;
    assert_eq!((s, g.is_object()), (StatusCode::FORBIDDEN, true));
    let (s, g) = call(&app, "PUT", &format!("/api/v1/households/{}/goal", f.hid), Some(&f.alex), Some(json!({ "target": 30, "reward": "Eis essen", "every_period": false }))).await;
    assert_eq!((s, g["target"].as_i64(), g["reward"].as_str()), (StatusCode::OK, Some(30), Some("Eis essen")));
    let (_, rules) = call(&app, "GET", &format!("/api/v1/households/{}/rules", f.hid), Some(&f.alex), None).await;
    assert!(rules["goal_policy"]["target"].is_null());
    // A catch-up template to pick from.
    call(&app, "POST", &format!("/api/v1/households/{}/templates", f.hid), Some(&f.alex), Some(json!({
        "title": "Keller aufräumen", "kind": "penalty", "points": { "min": 5, "default": 5, "max": 5 },
        "schedule": { "type": "once" }, "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    }))).await;

    earn(&app, &f, &f.alex, "Auto waschen", 20).await;
    assert_eq!(period(&app, &f).await["goal"]["status"], "running");
    earn(&app, &f, &f.sam, "Fenster putzen", 10).await;
    let p = period(&app, &f).await;
    assert_eq!((p["goal"]["status"].as_str(), p["goal"]["family_xp"].as_i64()), (Some("won"), Some(30)), "{p}");

    // The month ends: the scheduler drafts the settlement and opens the next period.
    let first = p["id"].as_str().unwrap().to_string();
    end_period(&db, &state).await;
    let p = period(&app, &f).await;
    assert_ne!(p["id"], first.as_str(), "a new period runs");
    assert_eq!(p["goal"]["status"], "running");
    let s = &p["settlements"][0];
    assert_eq!((s["status"].as_str(), s["total_xp"].as_i64(), s["goal"]["status"].as_str()), (Some("draft"), Some(30), Some("won")));
    let line = |mid: &str| s["lines"].as_array().unwrap().iter().find(|l| l["member_id"] == mid).unwrap().clone();
    // Fair share of 30 by weight 1 / 1 / 0.3: 13 / 13 / 4.
    let (alex, sam, mia) = (line(&f.alex_mid), line(&f.sam_mid), line(&f.mia_mid));
    assert_eq!((alex["outcome"].as_str(), alex["balance_delta"].as_i64()), (Some("surplus"), Some(4)));
    assert_eq!((sam["outcome"].as_str(), sam["penalty_points_due"].as_i64()), (Some("deficit"), Some(3)));
    assert_eq!((mia["outcome"].as_str(), mia["penalty_points_due"].as_i64()), (Some("deficit"), Some(4)));

    // Only admins approve, and only once.
    let sid = s["id"].as_str().unwrap().to_string();
    let (st, _) = call(&app, "POST", &format!("/api/v1/settlements/{sid}/approve"), Some(&f.mia), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    let before = balance(&app, &f, &f.alex_mid).await;
    let (st, s) = call(&app, "POST", &format!("/api/v1/settlements/{sid}/approve"), Some(&f.sam), None).await;
    assert_eq!((st, s["status"].as_str(), s["approved_by"].as_str()), (StatusCode::OK, Some("approved"), Some(f.sam_mid.as_str())));
    let (st, _) = call(&app, "POST", &format!("/api/v1/settlements/{sid}/approve"), Some(&f.alex), None).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(balance(&app, &f, &f.alex_mid).await, before + 4, "bonus for the one ahead");

    // Catch-up tasks: XP but no G, assigned, due in a week.
    let (_, tasks) = call(&app, "GET", &format!("/api/v1/households/{}/tasks", f.hid), Some(&f.alex), None).await;
    let catch_up: Vec<&Value> = tasks.as_array().unwrap().iter().filter(|t| t["kind"] == "penalty" && !t["assignee_id"].is_null()).collect();
    let owners: Vec<&str> = catch_up.iter().map(|t| t["assignee_id"].as_str().unwrap()).collect();
    assert!(owners.contains(&f.sam_mid.as_str()) && owners.contains(&f.mia_mid.as_str()), "{catch_up:?}");
    assert!(catch_up.iter().all(|t| t["title"] == "Keller aufräumen" && t["status"] == "claimed" && t["xp"] == 5));
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn automatic_approval_and_carry_over(db: PgPool) {
    let state = AppState::new(db.clone(), SECRET);
    let app = routes::router(state.clone(), &[]);
    let f = family(&app).await;
    // Household default: deficit and lead carry over, auto-approval after 24 h.
    earn(&app, &f, &f.alex, "Einkaufen", 30).await;
    end_period(&db, &state).await;
    let p = period(&app, &f).await;
    let s = &p["settlements"][0];
    assert_eq!(s["status"], "draft");
    assert!(s["auto_approve_at"].is_string());
    let sam = s["lines"].as_array().unwrap().iter().find(|l| l["member_id"] == f.sam_mid.as_str()).unwrap();
    assert_eq!((sam["outcome"].as_str(), sam["carry_next"].as_i64()), (Some("deficit"), Some(13)), "Sam's gap moves on");

    // 24 hours later the scheduler approves it by itself.
    sqlx::query("UPDATE jobs SET run_at = now() WHERE kind = 'approve_settlement'").execute(&db).await.unwrap();
    assert_eq!(scheduler::run_due(&state).await.unwrap(), 1);
    let p = period(&app, &f).await;
    let s = &p["settlements"][0];
    assert_eq!((s["status"].as_str(), s["approved_by"].is_null()), (Some("approved"), true));

    // Next month Sam's target includes the carried 13 XP: with nothing done, he stays behind.
    end_period(&db, &state).await;
    let p = period(&app, &f).await;
    let sam = p["settlements"][0]["lines"].as_array().unwrap().iter().find(|l| l["member_id"] == f.sam_mid.as_str()).unwrap().clone();
    assert_eq!((sam["target"].as_i64(), sam["contribution"].as_i64()), (Some(13), Some(0)));
}

/// Overdue tasks push Krümel: he can arrive before the family does.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn krumel_wins_when_tasks_lie_around(db: PgPool) {
    let state = AppState::new(db.clone(), SECRET);
    let app = routes::router(state.clone(), &[]);
    let f = family(&app).await;
    set_rules(&app, &f, |r| r["goal_policy"]["target"] = json!(40)).await;
    // A long period that started 10 days ago; a 20-XP task overdue for all 10 days.
    sqlx::query("UPDATE periods SET starts_at = now() - interval '10 days', ends_at = now() + interval '1000 days' WHERE status = 'running'")
        .execute(&db)
        .await
        .unwrap();
    let due = (chrono::Utc::now() - chrono::Duration::days(10)).to_rfc3339();
    call(&app, "POST", &format!("/api/v1/households/{}/templates", f.hid), Some(&f.alex), Some(json!({
        "title": "Garage", "points": { "min": 20, "default": 20, "max": 20 }, "schedule": { "type": "once" },
        "assignment": { "type": "open" }, "steps": [], "step_order": "free", "due_at": due
    }))).await;
    // 10 days × 20 XP × 25 % = 50 ≥ 40: Krümel is there first.
    let p = period(&app, &f).await;
    assert_eq!((p["goal"]["status"].as_str(), p["goal"]["krumel_xp"].as_i64()), (Some("lost"), Some(40)), "{p}");
    // Reaching the goal afterwards does not change the result.
    earn(&app, &f, &f.alex, "Großputz", 50).await;
    assert_eq!(period(&app, &f).await["goal"]["status"], "lost");
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn rewards(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let f = family(&app).await;
    let path = format!("/api/v1/households/{}/rewards", f.hid);
    let (s, _) = call(&app, "POST", &path, Some(&f.mia), Some(json!({ "title": "Alles", "cost": 1, "needs_approval": false }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "children do not create rewards");
    let (s, film) = call(&app, "POST", &path, Some(&f.alex),
        Some(json!({ "title": "Film aussuchen", "cost": 10, "needs_approval": false, "cooldown_hours": 24 }))).await;
    assert_eq!(s, StatusCode::CREATED, "{film}");
    let (_, ice) = call(&app, "POST", &path, Some(&f.alex),
        Some(json!({ "title": "Eis", "cost": 5, "needs_approval": true, "stock": 1 }))).await;

    earn(&app, &f, &f.mia, "Zimmer aufräumen", 15).await;
    assert_eq!(balance(&app, &f, &f.mia_mid).await, 15);

    // Film: straight away, then a cooldown.
    let (s, r) = call(&app, "POST", &format!("/api/v1/rewards/{}/redeem", film["id"].as_str().unwrap()), Some(&f.mia), None).await;
    assert_eq!((s, r["status"].as_str()), (StatusCode::CREATED, Some("approved")));
    assert_eq!(balance(&app, &f, &f.mia_mid).await, 5);
    let (s, _) = call(&app, "POST", &format!("/api/v1/rewards/{}/redeem", film["id"].as_str().unwrap()), Some(&f.mia), None).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (_, list) = call(&app, "GET", &path, Some(&f.mia), None).await;
    assert!(list.as_array().unwrap().iter().any(|r| r["title"] == "Film aussuchen" && r["available_at"].is_string()));

    // Eis: waits for an admin; the only one in stock.
    let ice_id = ice["id"].as_str().unwrap();
    let (s, r) = call(&app, "POST", &format!("/api/v1/rewards/{ice_id}/redeem"), Some(&f.mia), None).await;
    assert_eq!((s, r["status"].as_str()), (StatusCode::CREATED, Some("requested")));
    assert_eq!(balance(&app, &f, &f.mia_mid).await, 0);
    let (s, _) = call(&app, "POST", &format!("/api/v1/rewards/{ice_id}/redeem"), Some(&f.sam), None).await;
    assert_eq!(s, StatusCode::CONFLICT, "out of stock");
    let (_, waiting) = call(&app, "GET", &format!("/api/v1/households/{}/redemptions", f.hid), Some(&f.alex), None).await;
    assert_eq!(waiting.as_array().unwrap().len(), 1);

    // Rejected: the G come back, and the stock is free again.
    let rid = r["id"].as_str().unwrap();
    let (s, r) = call(&app, "POST", &format!("/api/v1/redemptions/{rid}/decide"), Some(&f.alex), Some(json!({ "approve": false }))).await;
    assert_eq!((s, r["status"].as_str()), (StatusCode::OK, Some("rejected")));
    assert_eq!(balance(&app, &f, &f.mia_mid).await, 5);
    let (s, _) = call(&app, "POST", &format!("/api/v1/redemptions/{rid}/decide"), Some(&f.alex), Some(json!({ "approve": true }))).await;
    assert_eq!(s, StatusCode::CONFLICT);
    let (_, list) = call(&app, "GET", &path, Some(&f.mia), None).await;
    assert_eq!(list.as_array().unwrap().iter().find(|r| r["title"] == "Eis").unwrap()["left"], 1);

    // Archived rewards disappear from the catalog.
    let (s, _) = call(&app, "DELETE", &format!("{path}/{ice_id}"), Some(&f.alex), None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, list) = call(&app, "GET", &path, Some(&f.mia), None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn rules_are_for_admins_and_checked(db: PgPool) {
    let app = routes::router(AppState::new(db, SECRET), &[]);
    let f = family(&app).await;
    let path = format!("/api/v1/households/{}/rules", f.hid);
    let (s, rules) = call(&app, "GET", &path, Some(&f.mia), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(rules["period_policy"]["tolerance_percent"], 20);
    let (s, _) = call(&app, "PUT", &path, Some(&f.mia), Some(rules.clone())).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let mut bad = rules.clone();
    bad["point_policy"]["task_min"] = json!(0);
    let (s, _) = call(&app, "PUT", &path, Some(&f.alex), Some(bad)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let mut bad = rules.clone();
    bad["goal_policy"]["reward"] = json!(" ");
    let (s, _) = call(&app, "PUT", &path, Some(&f.alex), Some(bad)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // Approval for everyone: now Sam's tasks wait too.
    let mut all = rules;
    all["point_policy"]["approval"] = json!("everyone");
    let (s, _) = call(&app, "PUT", &path, Some(&f.alex), Some(all)).await;
    assert_eq!(s, StatusCode::OK);
    let (_, b) = call(&app, "POST", &format!("/api/v1/households/{}/templates", f.hid), Some(&f.alex), Some(json!({
        "title": "Kochen", "schedule": { "type": "once" }, "assignment": { "type": "open" }, "steps": [], "step_order": "free"
    }))).await;
    let (_, r) = call(&app, "POST", &format!("/api/v1/tasks/{}/action", b["task"]["id"].as_str().unwrap()), Some(&f.sam), Some(json!({ "action": "done" }))).await;
    assert_eq!(r["submitted"], true);
}
