//! Background jobs: in-process tokio tasks. Each one is safe to run on several
//! server instances at once, because its work is done under per-user advisory
//! locks and is idempotent.

use std::time::Duration;

use sqlx::PgPool;

use crate::db::sync::purge_trash;

/// Items stay in the trash this many days before they are deleted for good.
pub const TRASH_DAYS: i32 = 30;

/// How often the trash is checked.
const TRASH_EVERY: Duration = Duration::from_secs(60 * 60);

/// Starts every job. They stop when the process does.
pub fn spawn(db: PgPool) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(TRASH_EVERY);
        loop {
            tick.tick().await;
            match purge_trash(&db, TRASH_DAYS).await {
                Ok(0) => {}
                Ok(count) => tracing::info!(count, "deleted items that were 30 days in the trash"),
                Err(error) => tracing::warn!(%error, "the trash purge failed"),
            }
        }
    });
}
