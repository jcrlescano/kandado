//! The Kandado backend. It authenticates users, stores ciphertext it cannot
//! read, and orders changes for sync. See `docs/architecture/backend.md`.
#![forbid(unsafe_code)]

mod auth;
mod config;
mod db;
mod error;
mod events;
mod http;
mod jobs;

use std::{process::ExitCode, time::Duration};

use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;

use crate::config::{Config, LogFormat};

/// Embedded at build time from `src/backend/migrations`.
static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!("../migrations");

#[tokio::main]
async fn main() -> ExitCode {
    // A missing .env file is normal outside local development.
    let _ = dotenvy::dotenv();

    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("configuration error: {error}");
            return ExitCode::from(2);
        }
    };
    init_logging(config.log_format);

    match run(config).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "server stopped");
            ExitCode::FAILURE
        }
    }
}

async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let db = PgPoolOptions::new()
        .max_connections(config.db_max_connections)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&config.database_url)
        .await?;
    MIGRATIONS.run(&db).await?;
    tracing::info!("database ready");

    let state = http::AppState::new(db.clone(), config.clone());
    state.hub.listen(db.clone());
    jobs::spawn(db.clone());

    let listener = TcpListener::bind(config.bind).await?;
    tracing::info!(address = %listener.local_addr()?, "listening");

    axum::serve(listener, http::router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    db.close().await;
    tracing::info!("stopped");
    Ok(())
}

fn init_logging(format: LogFormat) {
    // RUST_LOG overrides the level, for example RUST_LOG=kandado_server=debug.
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match format {
        LogFormat::Json => builder.json().init(),
        LogFormat::Text => builder.init(),
    }
}

/// Resolves on Ctrl+C, or on SIGTERM where that exists, so in-flight
/// requests finish before the process exits.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {}
        () = terminate => {}
    }
    tracing::info!("shutting down");
}
