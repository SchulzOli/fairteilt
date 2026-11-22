//! Background job loop on top of the `jobs` table.
//!
//! Jobs are claimed with `FOR UPDATE SKIP LOCKED`, so several server instances could run
//! side by side later. Handlers for reminders, recurrence expansion and period settlement
//! are added per milestone (M2–M5); unknown kinds fail and are retried with backoff.

use std::time::Duration;

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

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

pub async fn run(db: PgPool) {
    tracing::info!("scheduler started");
    loop {
        match claim(&db).await {
            Ok(Some(job)) => {
                let result = dispatch(&db, &job).await;
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

async fn dispatch(_db: &PgPool, job: &Job) -> anyhow::Result<()> {
    match job.kind.as_str() {
        "noop" => {
            tracing::debug!(payload = %job.payload, "noop job");
            Ok(())
        }
        // M2: "expand_recurrence", M5: "send_reminder", M4: "close_period" …
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
