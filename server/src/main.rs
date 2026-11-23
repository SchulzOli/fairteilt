//! punkte-server binary: config, database, migrations, scheduler, HTTP.

use std::time::Duration;

use anyhow::Context;
use punkte_server::config::Config;
use punkte_server::{AppState, MIGRATOR, routes, scheduler};
use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| "punkte_server=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let db = connect_with_retry(&config.database_url).await?;

    MIGRATOR.run(&db).await.context("running migrations")?;
    tracing::info!("migrations up to date");

    let mut state = AppState::new(db, config.jwt_secret.as_bytes());
    state.trust_proxy = config.trust_proxy;
    match &config.fcm_service_account {
        Some(path) => {
            state.push = Some(std::sync::Arc::new(punkte_server::push::Fcm::from_service_account_file(path)?));
            tracing::info!("push over FCM enabled");
        }
        None => tracing::info!("FCM_SERVICE_ACCOUNT not set: no push, inbox only"),
    }
    if config.google_client_ids.is_empty() && config.google_desktop.is_none() {
        tracing::info!("GOOGLE_CLIENT_IDS not set: sign in with Google is off");
    } else {
        let mut google = punkte_server::google::GoogleAuth::new(config.google_client_ids.clone());
        if let Some(desktop) = config.google_desktop.clone() {
            google = google.with_desktop(desktop);
            tracing::info!("sign in with Google enabled (also in the desktop app)");
        } else {
            tracing::info!("sign in with Google enabled");
        }
        state.google = std::sync::Arc::new(google);
    }
    let scheduler = tokio::spawn(scheduler::run(state.clone()));

    let listener = TcpListener::bind(&config.bind_addr)
        .await
        .with_context(|| format!("binding {}", config.bind_addr))?;
    tracing::info!("listening on http://{}", config.bind_addr);

    let app = routes::router(state, &config.cors_origins);
    axum::serve(listener, app.into_make_service_with_connect_info::<std::net::SocketAddr>())
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
