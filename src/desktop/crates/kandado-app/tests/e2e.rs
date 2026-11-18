//! Phase 1's exit criteria, end to end: two computers with the same Google
//! account, one master password, revoking one from the other, and recovery
//! with the phrase after "forgetting" the password.
//!
//! It runs the real `kandado-server` against PostgreSQL. Google is replaced by
//! the server's development sign-in route; everything after Google is the real
//! flow: the loopback listener, the code exchange, key creation and unlock.
//!
//! ```text
//! docker compose -f src/backend/docker-compose.yml up -d
//! cargo build -p kandado-server
//! DATABASE_URL=postgres://kandado:kandado@127.0.0.1:54329/kandado \
//!   cargo test -p kandado-app --test e2e -- --ignored
//! ```

mod common;

use std::sync::Arc;

use common::{Server, computer, google, start_server, try_google, unique_email};
use kandado_app::{
    KandadoApp, Outcome, Paths, Phase, UnlockError, device_key::MemoryDeviceKey,
    secrets::MemorySecrets,
};
use kandado_crypto::kdf::KdfParams;
use kandado_vault::ApiError;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the server binary and PostgreSQL; see the module docs"]
async fn two_computers_one_account() {
    let server = start_server().await;
    let email = unique_email();
    let password = "correct horse battery staple";

    // --- Computer A: first sign-in, master password, recovery phrase ---
    let a = computer("a", &server);
    assert_eq!(a.restore().await.unwrap(), Phase::SignedOut);
    let phase = google(&a, &server, &email).await.unwrap();
    assert_eq!(
        phase,
        Phase::NeedsMasterPassword {
            email: email.clone()
        }
    );

    // A weak password is refused before anything is derived or sent.
    assert!(a.create_master_password("12345678".into()).await.is_err());
    let words = a.create_master_password(password.into()).await.unwrap();
    assert_eq!(words.len(), 12);
    let phrase = words.join(" ");
    let phase = a.confirm_recovery_saved().await.unwrap();
    assert_eq!(
        phase,
        Phase::Unlocked {
            email: email.clone()
        }
    );
    assert!(a.is_unlocked());

    // Locking drops the key; the password brings it back.
    a.lock(kandado_app::lock::LockReason::Manual);
    assert!(!a.is_unlocked());
    assert!(matches!(
        a.unlock_with_password("not the password".into()).await,
        Err(UnlockError::WrongPassword { tries_left: 4 })
    ));
    a.unlock_with_password(password.into()).await.unwrap();
    assert!(a.is_unlocked());

    // --- Computer B: same Google account, same master password ---
    let b = computer("b", &server);
    let phase = google(&b, &server, &email).await.unwrap();
    assert_eq!(
        phase,
        Phase::Locked {
            email: email.clone(),
            hello: false
        }
    );
    // Five wrong passwords make it wait.
    for tries_left in [4, 3, 2, 1, 0] {
        assert!(matches!(
            b.unlock_with_password("wrong".into()).await,
            Err(UnlockError::WrongPassword { tries_left: left }) if left == tries_left
        ));
    }
    assert!(matches!(
        b.unlock_with_password(password.into()).await,
        Err(UnlockError::Wait(_))
    ));
    // A restart forgets the wait (it is only in memory), so use a fresh
    // object over the same files, as relaunching the app would.
    drop(b);
    let b = relaunch("b", &server);
    let phase = b.unlock_with_password(password.into()).await.unwrap();
    assert_eq!(
        phase,
        Phase::Unlocked {
            email: email.clone()
        }
    );

    // --- Devices: A sees both and signs B out ---
    let devices = a.devices().await.unwrap().devices;
    assert_eq!(devices.len(), 2);
    assert_eq!(devices.iter().filter(|d| d.current).count(), 1);
    let other = devices.iter().find(|d| !d.current).unwrap();
    a.revoke_device(other.id).await.unwrap();
    assert_eq!(a.devices().await.unwrap().devices.len(), 1);
    // B finds out the next time it talks to the server.
    let error = b.devices().await.unwrap_err();
    assert!(
        matches!(error, kandado_app::AppError::Api(ApiError::SignedOut)),
        "{error}"
    );

    // --- Forgot the password: recover on a third computer with the phrase ---
    let c = computer("c", &server);
    google(&c, &server, &email).await.unwrap();
    let new_password = "a brand new master password";
    // Wrong words are refused.
    let wrong = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
    assert!(matches!(
        c.recover(wrong.into(), new_password.into()).await,
        Err(kandado_app::AppError::WrongPhrase)
    ));
    assert_eq!(
        c.recover(phrase.clone(), new_password.into())
            .await
            .unwrap(),
        Outcome::Done
    );
    assert!(c.is_unlocked());

    // The new password works on a fresh computer; the old one does not.
    let d = computer("d", &server);
    google(&d, &server, &email).await.unwrap();
    assert!(matches!(
        d.unlock_with_password(password.into()).await,
        Err(UnlockError::WrongPassword { .. })
    ));
    d.unlock_with_password(new_password.into()).await.unwrap();

    // --- Another Google account cannot take over a computer that has an account ---
    let intruder = unique_email();
    let error = try_google(&a, &server, &intruder).await.unwrap_err();
    assert!(
        matches!(
            error,
            kandado_app::AppError::Auth(kandado_app::auth::AuthError::WrongAccount)
        ),
        "{error}"
    );
    assert_eq!(a.email().as_deref(), Some(email.as_str()));

    // --- Sign out wipes the computer ---
    d.sign_out().await.unwrap();
    assert_eq!(d.restore().await.unwrap(), Phase::SignedOut);
    assert!(d.email().is_none());
    assert!(!d.is_unlocked());
    // The server forgot the device too.
    let remaining = c.devices().await.unwrap().devices.len();
    assert!(remaining >= 1, "{remaining}");
}

/// A new process over the same data folder (the secret store is memory, so
/// the refresh token does not survive, as if the Windows credential was lost).
fn relaunch(name: &str, server: &Server) -> Arc<KandadoApp> {
    let dir = std::env::temp_dir().join(format!(
        "kandado-e2e-{}-{name}-{}",
        std::process::id(),
        server.url.rsplit(':').next().unwrap()
    ));
    KandadoApp::with_params(
        Paths { data_dir: dir },
        Arc::new(MemorySecrets::default()),
        Arc::new(MemoryDeviceKey::new()),
        KdfParams::MIN,
        Some(&server.url),
    )
    .unwrap()
}
