//! punkte-server: REST API, scheduler and persistence.

mod config;
mod error;
mod routes;
mod scheduler;

use std::time::Duration;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

use crate::config::Config;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "punkte_server=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let db = connect_with_retry(&config.database_url).await?;

    sqlx::migrate!("./migrations").run(&db).await.context("running migrations")?;
    tracing::info!("migrations up to date");

    let state = AppState { db };
    let scheduler = tokio::spawn(scheduler::run(state.db.clone()));

    let listener = TcpListener::bind(&config.bind_addr)
        .await
        .with_context(|| format!("binding {}", config.bind_addr))?;
    tracing::info!("listening on http://{}", config.bind_addr);

    axum::serve(listener, routes::router(state, &config))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    scheduler.abort();
    Ok(())
}

/// Postgres may come up after the API container; retry for ~30 s.
async fn connect_with_retry(url: &str) -> anyhow::Result<sqlx::PgPool> {
    let mut attempt = 0;
    loop {
        match PgPoolOptions::new().max_connections(10).acquire_timeout(Duration::from_secs(5)).connect(url).await {
            Ok(pool) => return Ok(pool),
            Err(err) if attempt < 15 => {
                attempt += 1;
                tracing::warn!(attempt, %err, "database not reachable yet, retrying");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            Err(err) => return Err(err).context("connecting to database"),
        }
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down");
}
