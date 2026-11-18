//! "Confirm it's you": the fresh Google sign-in that sensitive actions need.

use kandado_app::{AppError, auth::AuthError, platform};

use super::super::Services;

/// Opens Google in the browser and waits for the person to come back.
/// Afterwards the session counts as freshly signed in.
pub async fn google(services: &Services) -> Result<(), AppError> {
    let app = services.app.clone();
    services
        .tasks
        .run(async move {
            let pending = app.begin_google().await?;
            platform::open_url(pending.url.as_str())
                .map_err(|_| AppError::Auth(AuthError::Failed))?;
            app.finish_google(pending).await?;
            Ok(())
        })
        .await
}
