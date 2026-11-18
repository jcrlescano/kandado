//! The browser extension link (D-45) through the application service: pairing
//! with the code, and the user key sealed to a paired extension, opened the way
//! the extension opens it. No browser and no pipe: the same answers the pipe
//! server gets (`kandado-bridge`'s own tests cover the pipe and the host).

use std::sync::Arc;

use kandado_app::{
    KandadoApp, KdfParams, Paths, Phase,
    browser::{self, BrowserLink},
    device_key::MemoryDeviceKey,
    lock::LockReason,
    secrets::MemorySecrets,
    testing::seed_account,
};
use kandado_bridge::protocol::{AppState, Refusal, ResponseBody};
use kandado_crypto::{account, pairing, seal};
use uuid::Uuid;

const PASSWORD: &str = "tubig at langis sa kusina";

struct Fixture {
    dir: std::path::PathBuf,
    app: Arc<KandadoApp>,
    link: Arc<BrowserLink>,
}

async fn unlocked(name: &str) -> Fixture {
    let dir = std::env::temp_dir().join(format!("kandado-app-link-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let paths = Paths {
        data_dir: dir.clone(),
    };
    seed_account(&paths, "juan@example.test", PASSWORD);
    let app = KandadoApp::with_params(
        paths,
        Arc::new(MemorySecrets::default()),
        Arc::new(MemoryDeviceKey::new()),
        KdfParams::MIN,
        Some("http://127.0.0.1:9"),
    )
    .unwrap();
    assert!(matches!(app.restore().await.unwrap(), Phase::Locked { .. }));
    app.unlock_with_password(PASSWORD.to_owned()).await.unwrap();
    let link = BrowserLink::new(app.clone(), tokio::runtime::Handle::current(), dir.clone());
    Fixture { dir, app, link }
}

/// The user key, the way this computer derives it from the master password.
fn user_key(fixture: &Fixture) -> Vec<u8> {
    let store = kandado_vault::Store::open(
        &Paths {
            data_dir: fixture.dir.clone(),
        }
        .vault_db(),
    )
    .unwrap();
    let local = store.account().unwrap().unwrap();
    account::unlock_with_password(*local.user_id.as_bytes(), &local.bundle, PASSWORD)
        .unwrap()
        .expose()
        .to_vec()
}

#[tokio::test]
async fn a_paired_extension_gets_the_user_key_only_it_can_open() {
    let fixture = unlocked("pair").await;
    let handler = browser::handler_for_tests(&fixture.link);

    // Off: nothing pairs and nothing unlocks.
    let (private, public) = seal::generate_keypair().unwrap();
    let pairing_id = Uuid::from_u128(42);
    assert_eq!(
        handler
            .pair(
                "chrome-extension://x/".into(),
                pairing_id,
                *public.as_bytes(),
                "Edge".into()
            )
            .await,
        ResponseBody::Refused {
            reason: Refusal::TurnedOff
        }
    );

    browser::enable_for_tests(&fixture.link).unwrap();
    let user_id = fixture.app.user_id().unwrap();
    assert_eq!(
        handler.status(),
        ResponseBody::Status {
            version: env!("CARGO_PKG_VERSION").into(),
            state: AppState::Unlocked,
            user_id: Some(user_id),
            unlock_enabled: true,
        }
    );

    // The request waits for the person, who sees the code the extension shows.
    let waiting = handler.pair(
        "chrome-extension://x/".into(),
        pairing_id,
        *public.as_bytes(),
        "Edge".into(),
    );
    let request = fixture.link.pending().expect("a request to answer");
    assert_eq!(request.code, pairing::code(public.as_bytes()));
    assert_eq!(request.browser, "Edge");
    // A second request while one waits is refused.
    let (_, other) = seal::generate_keypair().unwrap();
    assert_eq!(
        handler
            .pair(
                "chrome-extension://y/".into(),
                Uuid::from_u128(43),
                *other.as_bytes(),
                "Chrome".into()
            )
            .await,
        ResponseBody::Refused {
            reason: Refusal::Declined
        }
    );
    fixture.link.answer(pairing_id, true).unwrap();
    assert_eq!(waiting.await, ResponseBody::Paired);
    assert_eq!(fixture.link.pairings().len(), 1);

    // Unlock: the sealed user key opens with the extension's private key, and is the real one.
    let ResponseBody::Unlocked { sealed } = handler.unlock(pairing_id) else {
        panic!("not unlocked");
    };
    let sealed = {
        use base64::{Engine, engine::general_purpose::STANDARD};
        STANDARD.decode(sealed).unwrap()
    };
    let opened = pairing::open_user_key(
        *user_id.as_bytes(),
        *pairing_id.as_bytes(),
        &private,
        &sealed,
    )
    .unwrap();
    assert_eq!(opened.expose().to_vec(), user_key(&fixture));

    // Unknown pairing, locked app, setting off: refused.
    assert_eq!(
        handler.unlock(Uuid::from_u128(7)),
        ResponseBody::Refused {
            reason: Refusal::NotPaired
        }
    );
    fixture.app.lock(LockReason::Manual);
    assert_eq!(
        handler.unlock(pairing_id),
        ResponseBody::Refused {
            reason: Refusal::Locked
        }
    );

    // Pairings survive a restart of the app.
    let again = BrowserLink::new(
        fixture.app.clone(),
        tokio::runtime::Handle::current(),
        fixture.dir.clone(),
    );
    assert_eq!(again.pairings().len(), 1);
    again.remove(pairing_id).unwrap();
    assert!(again.pairings().is_empty());
}

#[tokio::test]
async fn a_refused_or_locked_pairing_is_not_kept() {
    let fixture = unlocked("refuse").await;
    browser::enable_for_tests(&fixture.link).unwrap();
    let handler = browser::handler_for_tests(&fixture.link);
    let (_, public) = seal::generate_keypair().unwrap();

    let waiting = handler.pair(
        "o".into(),
        Uuid::from_u128(1),
        *public.as_bytes(),
        "Edge".into(),
    );
    fixture.link.answer(Uuid::from_u128(1), false).unwrap();
    assert_eq!(
        waiting.await,
        ResponseBody::Refused {
            reason: Refusal::Declined
        }
    );
    assert!(fixture.link.pairings().is_empty());

    fixture.app.lock(LockReason::Manual);
    assert_eq!(
        handler
            .pair(
                "o".into(),
                Uuid::from_u128(2),
                *public.as_bytes(),
                "Edge".into()
            )
            .await,
        ResponseBody::Refused {
            reason: Refusal::Locked
        }
    );
}
