//! M5: inbox for the right people, reminders and escalation, warning before the period ends,
//! push over FCM (against a local stand-in) with quiet hours and settings.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, StatusCode};
use http_body_util::BodyExt;
use punkte_server::push::Fcm;
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
    Family { alex, sam, mia, hid, mia_mid }
}

async fn inbox(app: &Router, f: &Family, token: &str) -> Vec<Value> {
    let (s, list) = call(app, "GET", &format!("/api/v1/households/{}/inbox", f.hid), Some(token), None).await;
    assert_eq!(s, StatusCode::OK, "{list}");
    list.as_array().unwrap().clone()
}

async fn titles(app: &Router, f: &Family, token: &str) -> Vec<String> {
    inbox(app, f, token).await.iter().map(|n| n["title"].as_str().unwrap().to_string()).collect()
}

async fn template(app: &Router, f: &Family, body: Value) -> String {
    let (s, b) = call(app, "POST", &format!("/api/v1/households/{}/templates", f.hid), Some(&f.alex), Some(body)).await;
    assert_eq!(s, StatusCode::CREATED, "{b}");
    b["task"]["id"].as_str().unwrap().to_string()
}

/// The M5 scenario: everybody hears what concerns them; reminders when a task gets due and
/// overdue (then the admins); a warning before the month ends.
#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn m5_inbox_and_reminders(db: PgPool) {
    let state = AppState::new(db.clone(), SECRET);
    let app = routes::router(state.clone(), &[]);
    let f = family(&app).await;

    // Assigned → Mia hears about it (Alex, who created it, does not).
    let tid = template(&app, &f, json!({
        "title": "Zimmer aufräumen", "schedule": { "type": "once" }, "assignment": { "type": "fixed", "member": f.mia_mid },
        "steps": [], "step_order": "free"
    }))
    .await;
    assert_eq!(titles(&app, &f, &f.mia).await, ["Neue Aufgabe für dich: „Zimmer aufräumen“"]);
    assert!(inbox(&app, &f, &f.alex).await.is_empty());

    // Done → both adults are asked to confirm; confirmed → Mia hears it, with the message.
    call(&app, "POST", &format!("/api/v1/tasks/{tid}/action"), Some(&f.mia), Some(json!({ "action": "done" }))).await;
    assert_eq!(titles(&app, &f, &f.alex).await, ["Mia ist fertig: „Zimmer aufräumen“"]);
    assert_eq!(titles(&app, &f, &f.sam).await, ["Mia ist fertig: „Zimmer aufräumen“"]);
    call(&app, "POST", &format!("/api/v1/tasks/{tid}/review"), Some(&f.sam), Some(json!({ "approve": true, "note": "Super!" }))).await;
    let mine = inbox(&app, &f, &f.mia).await;
    assert_eq!(mine[0]["title"], "„Zimmer aufräumen“ bestätigt · +10 XP · +10 G");
    assert_eq!((mine[0]["body"].as_str(), mine[0]["ref_type"].as_str(), mine[0]["ref_id"].as_str()), (Some("„Super!“ – Sam"), Some("task"), Some(tid.as_str())));

    // Unread count and marking as read.
    let (_, u) = call(&app, "GET", &format!("/api/v1/households/{}/inbox/unread", f.hid), Some(&f.mia), None).await;
    assert_eq!(u["unread"], 2);
    let (_, u) = call(&app, "POST", &format!("/api/v1/households/{}/inbox/read", f.hid), Some(&f.mia), Some(json!({ "ids": [mine[0]["id"]] }))).await;
    assert_eq!(u["unread"], 1);
    let (_, u) = call(&app, "POST", &format!("/api/v1/households/{}/inbox/read", f.hid), Some(&f.mia), Some(json!({ "ids": null }))).await;
    assert_eq!(u["unread"], 0);

    // Reminders: a weekly task at day 6 of 7 → "bald fällig" for Mia, once.
    let weekly = template(&app, &f, json!({
        "title": "Bad putzen", "schedule": { "type": "interval", "days": 7 }, "assignment": { "type": "fixed", "member": f.mia_mid },
        "steps": [], "step_order": "free"
    }))
    .await;
    sqlx::query("UPDATE tasks SET cycle_start = now() - interval '6 days', due_at = now() + interval '1 day' WHERE id = $1::uuid")
        .bind(&weekly)
        .execute(&db)
        .await
        .unwrap();
    scheduler::ensure_recurring(&db).await.unwrap();
    scheduler::run_due(&state).await.unwrap();
    assert_eq!(titles(&app, &f, &f.mia).await[0], "„Bad putzen“ ist bald fällig");
    let count = inbox(&app, &f, &f.mia).await.len();
    sqlx::query("UPDATE jobs SET run_at = now() WHERE kind = 'reminders' AND done_at IS NULL").execute(&db).await.unwrap();
    scheduler::run_due(&state).await.unwrap();
    assert_eq!(inbox(&app, &f, &f.mia).await.len(), count, "only once per round");

    // Two days overdue: Mia is reminded, the admins hear about it too.
    sqlx::query("UPDATE tasks SET cycle_start = now() - interval '9 days', due_at = now() - interval '2 days' WHERE id = $1::uuid")
        .bind(&weekly)
        .execute(&db)
        .await
        .unwrap();
    sqlx::query("UPDATE jobs SET run_at = now() WHERE kind = 'reminders' AND done_at IS NULL").execute(&db).await.unwrap();
    scheduler::run_due(&state).await.unwrap();
    assert_eq!(titles(&app, &f, &f.mia).await[0], "„Bad putzen“ ist überfällig");
    assert_eq!(titles(&app, &f, &f.alex).await[0], "„Bad putzen“ ist seit einem Tag überfällig");
    assert_eq!(inbox(&app, &f, &f.sam).await[0]["body"], "Mia ist dran.");

    // 48 hours before the end of the period: the ones behind get a hint (Mia did her share).
    sqlx::query("UPDATE jobs SET run_at = now() WHERE kind = 'warn_period'").execute(&db).await.unwrap();
    scheduler::run_due(&state).await.unwrap();
    for adult in [&f.alex, &f.sam] {
        let n = inbox(&app, &f, adult).await;
        assert!(n[0]["title"].as_str().unwrap().ends_with("bis zum Abschluss"), "{n:?}");
        assert!(n[0]["body"].as_str().unwrap().starts_with("Dir fehlen etwa"));
    }
    assert!(!titles(&app, &f, &f.mia).await.iter().any(|t| t.ends_with("bis zum Abschluss")));

    // Others' inboxes stay private.
    let (s, _) = call(&app, "GET", &format!("/api/v1/households/{}/inbox", f.hid), Some(&register(&app, "eve@example.org", "Eve").await), None).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[derive(Clone, Default)]
struct FakeFcm {
    received: Arc<Mutex<Vec<(String, Value)>>>,
}

/// Stand-in for `fcm.googleapis.com`: records messages, answers 404 UNREGISTERED for "gone".
async fn fake_fcm() -> (String, FakeFcm) {
    let fake = FakeFcm::default();
    let seen = fake.clone();
    let app = Router::new().route(
        "/send",
        axum::routing::post(move |headers: HeaderMap, axum::Json(body): axum::Json<Value>| {
            let seen = seen.clone();
            async move {
                let auth = headers.get("authorization").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                let gone = body["message"]["token"] == "gone";
                seen.received.lock().unwrap().push((auth, body));
                if gone {
                    (StatusCode::NOT_FOUND, r#"{"error":{"status":"NOT_FOUND","details":[{"errorCode":"UNREGISTERED"}]}}"#)
                } else {
                    (StatusCode::OK, r#"{"name":"projects/test/messages/1"}"#)
                }
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{addr}/send"), fake)
}

#[sqlx::test(migrator = "punkte_server::MIGRATOR")]
async fn push_over_fcm(db: PgPool) {
    let (endpoint, fake) = fake_fcm().await;
    let mut state = AppState::new(db.clone(), SECRET);
    state.push = Some(Arc::new(Fcm::with_static_token(endpoint, "test-token".into()).unwrap()));
    let app = routes::router(state.clone(), &[]);
    let f = family(&app).await;

    // Mia's own phone (and an old token that no longer exists).
    for token in ["mia-phone", "gone"] {
        let (s, _) = call(&app, "POST", "/api/v1/devices", Some(&f.mia), Some(json!({ "platform": "android", "push_provider": "fcm", "push_token": token }))).await;
        assert_eq!(s, StatusCode::NO_CONTENT);
    }
    let (s, _) = call(&app, "POST", "/api/v1/devices", Some(&f.mia), Some(json!({ "platform": "fridge", "push_provider": "fcm", "push_token": "x" }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);

    let tid = template(&app, &f, json!({
        "title": "Tisch decken", "schedule": { "type": "once" }, "assignment": { "type": "fixed", "member": f.mia_mid },
        "steps": [], "step_order": "free"
    }))
    .await;
    assert_eq!(scheduler::run_due(&state).await.unwrap(), 1, "one send_push job");
    {
        let got = fake.received.lock().unwrap();
        assert_eq!(got.len(), 2);
        let (auth, msg) = got.iter().find(|(_, m)| m["message"]["token"] == "mia-phone").unwrap();
        assert_eq!(auth, "Bearer test-token");
        assert_eq!(msg["message"]["notification"]["title"], "Neue Aufgabe für dich: „Tisch decken“");
        assert_eq!((msg["message"]["data"]["ref_type"].as_str(), msg["message"]["data"]["ref_id"].as_str()), (Some("task"), Some(tid.as_str())));
        assert_eq!(msg["message"]["android"]["notification"]["channel_id"], "fairteilt");
    }
    let tokens: Vec<String> = sqlx::query_scalar("SELECT push_token FROM devices").fetch_all(&db).await.unwrap();
    assert_eq!(tokens, ["mia-phone"], "the dead token is forgotten");
    let sent: bool = sqlx::query_scalar("SELECT push_sent_at IS NOT NULL FROM notifications WHERE ref_id = $1::uuid").bind(&tid).fetch_one(&db).await.unwrap();
    assert!(sent);

    // Quiet hours around "now" (household time): the push waits until they end.
    let now = chrono::Utc::now().with_timezone(&chrono_tz::Europe::Berlin);
    let from = (now - chrono::Duration::hours(1)).format("%H:%M").to_string();
    let to = (now + chrono::Duration::hours(1)).format("%H:%M").to_string();
    let prefs_path = format!("/api/v1/households/{}/notification-prefs", f.hid);
    let (_, mut prefs) = call(&app, "GET", &prefs_path, Some(&f.mia), None).await;
    assert_eq!((prefs["push"]["reviews"].as_bool(), prefs["push"]["reminders"].as_bool()), (Some(true), Some(false)));
    prefs["quiet_from"] = json!(from);
    prefs["quiet_to"] = json!(to);
    let (s, _) = call(&app, "PUT", &prefs_path, Some(&f.mia), Some(prefs.clone())).await;
    assert_eq!(s, StatusCode::OK);
    template(&app, &f, json!({
        "title": "Müll", "schedule": { "type": "once" }, "assignment": { "type": "fixed", "member": f.mia_mid }, "steps": [], "step_order": "free"
    }))
    .await;
    scheduler::run_due(&state).await.unwrap();
    assert_eq!(fake.received.lock().unwrap().len(), 2, "nothing during quiet hours");
    let later: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind = 'send_push' AND done_at IS NULL AND run_at > now()").fetch_one(&db).await.unwrap();
    assert_eq!(later, 1, "waits for the end of the quiet hours");

    // Category off: still in the inbox, no push job at all.
    prefs["quiet_from"] = Value::Null;
    prefs["quiet_to"] = Value::Null;
    prefs["push"]["tasks"] = json!(false);
    call(&app, "PUT", &prefs_path, Some(&f.mia), Some(prefs)).await;
    let jobs_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind = 'send_push'").fetch_one(&db).await.unwrap();
    template(&app, &f, json!({
        "title": "Blumen", "schedule": { "type": "once" }, "assignment": { "type": "fixed", "member": f.mia_mid }, "steps": [], "step_order": "free"
    }))
    .await;
    let jobs_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE kind = 'send_push'").fetch_one(&db).await.unwrap();
    assert_eq!(jobs_after, jobs_before);
    assert_eq!(titles(&app, &f, &f.mia).await[0], "Neue Aufgabe für dich: „Blumen“");

    // Bad quiet hours are refused; signing out of the phone removes the device.
    let (s, _) = call(&app, "PUT", &prefs_path, Some(&f.mia), Some(json!({ "quiet_from": "25:00", "quiet_to": "07:00", "push": { "tasks": true, "reviews": true, "points": true, "period": true, "reminders": false } }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    let (s, _) = call(&app, "DELETE", "/api/v1/devices", Some(&f.mia), Some(json!({ "push_token": "mia-phone" }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices").fetch_one(&db).await.unwrap();
    assert_eq!(left, 0);
    let _ = (&f.sam, &f.alex);
}
