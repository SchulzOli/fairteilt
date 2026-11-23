//! Background job loop on top of the `jobs` table.
//!
//! Jobs are claimed with `FOR UPDATE SKIP LOCKED`, so several server instances could run
//! side by side later. M4: `close_period` (end of a period: draft settlement, race decided)
//! and `approve_settlement` (automatic approval). Unknown kinds fail and are retried with backoff.

use std::time::Duration;

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::AppState;

const POLL_INTERVAL: Duration = Duration::from_secs(5);
const LOCK_SECS: i64 = 60;

#[derive(sqlx::FromRow)]
struct Job {
    id: Uuid,
    kind: String,
    payload: Value,
    attempts: i32,
    max_attempts: i32,
}

/// Makes sure the recurring jobs exist (reminder sweep every few minutes).
pub async fn ensure_recurring(db: &PgPool) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO jobs (id, kind, run_at) SELECT gen_random_uuid(), 'reminders', now()
         WHERE NOT EXISTS (SELECT 1 FROM jobs WHERE kind = 'reminders' AND done_at IS NULL)",
    )
    .execute(db)
    .await?;
    Ok(())
}

const REMINDER_EVERY_SECS: f64 = 300.0;

pub async fn run(state: AppState) {
    tracing::info!("scheduler started");
    let db = state.db.clone();
    if let Err(err) = ensure_recurring(&db).await {
        tracing::warn!(%err, "could not schedule reminders");
    }
    loop {
        match claim(&db).await {
            Ok(Some(job)) => {
                let result = dispatch(&state, &job).await;
                if let Err(err) = finish(&db, &job, result).await {
                    tracing::error!(%err, job = %job.id, "could not record job result");
                }
                continue; // more work may be waiting
            }
            Ok(None) => {}
            Err(err) => tracing::warn!(%err, "claiming jobs failed"),
        }
        tokio::time::sleep(POLL_INTERVAL).await;
    }
}

/// Runs every job that is due right now, then returns how many ran (tests, maintenance).
pub async fn run_due(state: &AppState) -> anyhow::Result<usize> {
    let mut n = 0;
    while let Some(job) = claim(&state.db).await? {
        let result = dispatch(state, &job).await;
        finish(&state.db, &job, result).await?;
        n += 1;
    }
    Ok(n)
}

async fn claim(db: &PgPool) -> sqlx::Result<Option<Job>> {
    sqlx::query_as::<_, Job>(
        r#"
        UPDATE jobs SET locked_until = now() + make_interval(secs => $1), attempts = attempts + 1
        WHERE id = (
            SELECT id FROM jobs
            WHERE done_at IS NULL
              AND run_at <= now()
              AND attempts < max_attempts
              AND (locked_until IS NULL OR locked_until < now())
            ORDER BY run_at
            FOR UPDATE SKIP LOCKED
            LIMIT 1
        )
        RETURNING id, kind, payload, attempts, max_attempts
        "#,
    )
    .bind(LOCK_SECS as f64)
    .fetch_optional(db)
    .await
}

fn id_in(payload: &Value, key: &str) -> anyhow::Result<Uuid> {
    let raw = payload.get(key).and_then(Value::as_str).ok_or_else(|| anyhow::anyhow!("payload without {key}"))?;
    Ok(raw.parse()?)
}

async fn dispatch(state: &AppState, job: &Job) -> anyhow::Result<()> {
    match job.kind.as_str() {
        "noop" => {
            tracing::debug!(payload = %job.payload, "noop job");
            Ok(())
        }
        "close_period" => {
            let period = id_in(&job.payload, "period_id")?;
            let hid: Option<Uuid> = sqlx::query_scalar("SELECT household_id FROM periods WHERE id = $1").bind(period).fetch_optional(&state.db).await?;
            if let Some(settlement) = crate::period::close_period(&state.db, period).await.map_err(|e| anyhow::anyhow!("{e:?}"))?
                && let Some(hid) = hid
            {
                tracing::info!(%period, %settlement, "period closed");
                state.live.notify_json(hid, serde_json::json!({ "type": "period_closed", "settlement_id": settlement }));
            }
            Ok(())
        }
        "approve_settlement" => {
            let settlement = id_in(&job.payload, "settlement_id")?;
            if crate::period::approve_settlement(&state.db, settlement, None).await.map_err(|e| anyhow::anyhow!("{e:?}"))? {
                let hid: Option<Uuid> = sqlx::query_scalar(
                    "SELECT p.household_id FROM settlements s JOIN periods p ON p.id = s.period_id WHERE s.id = $1",
                )
                .bind(settlement)
                .fetch_optional(&state.db)
                .await?;
                if let Some(hid) = hid {
                    tracing::info!(%settlement, "settlement approved automatically");
                    state.live.notify_json(hid, serde_json::json!({ "type": "settlement_approved", "settlement_id": settlement }));
                }
            }
            Ok(())
        }
        "send_push" => crate::inbox::deliver(state, id_in(&job.payload, "notification_id")?).await,
        "reminders" => {
            let n = crate::inbox::sweep_reminders(state).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
            crate::idempotency::prune(&state.db).await?;
            if n > 0 {
                tracing::info!(n, "reminders written");
            }
            // Next sweep in a few minutes (a new job, so this one can finish).
            sqlx::query("INSERT INTO jobs (id, kind, run_at) VALUES (gen_random_uuid(), 'reminders', now() + make_interval(secs => $1))")
                .bind(REMINDER_EVERY_SECS)
                .execute(&state.db)
                .await?;
            Ok(())
        }
        "warn_period" => {
            let n = crate::period::warn_period(&state.db, id_in(&job.payload, "period_id")?).await.map_err(|e| anyhow::anyhow!("{e:?}"))?;
            tracing::info!(n, "period warnings written");
            Ok(())
        }
        other => anyhow::bail!("no handler for job kind '{other}'"),
    }
}

async fn finish(db: &PgPool, job: &Job, result: anyhow::Result<()>) -> sqlx::Result<()> {
    match result {
        Ok(()) => {
            sqlx::query("UPDATE jobs SET done_at = now(), locked_until = NULL, last_error = NULL WHERE id = $1")
                .bind(job.id)
                .execute(db)
                .await?;
        }
        Err(err) => {
            // Exponential backoff: 30 s, 60 s, 120 s, …
            let backoff = 30.0 * 2f64.powi(job.attempts.saturating_sub(1).min(10));
            tracing::warn!(job = %job.id, kind = %job.kind, attempts = job.attempts, max = job.max_attempts, %err, "job failed");
            sqlx::query(
                "UPDATE jobs SET last_error = $2, locked_until = NULL, run_at = now() + make_interval(secs => $3) WHERE id = $1",
            )
            .bind(job.id)
            .bind(err.to_string())
            .bind(backoff)
            .execute(db)
            .await?;
        }
    }
    Ok(())
}
