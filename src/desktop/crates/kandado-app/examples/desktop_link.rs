//! Plays the Kandado desktop app for the extension's end-to-end test of the
//! desktop link (`src/extensions/tests/e2e/desktop-link.spec.ts`). Not part of
//! the app.
//!
//! It opens an account from the keys the test's server hands out, unlocks it,
//! and turns on "Unlock the browser extension with this app" (D-45), which
//! registers the native host with the browsers under `HKEY_CURRENT_USER`. It
//! accepts every pairing request and prints its code. Lines on stdin:
//! `lock`, `unlock`, `quit`. At the end it removes the registration.
//!
//! `cargo run -p kandado-app --example desktop_link -- <keys.json> <user id> <password> <data dir>`

use std::{io::BufRead, sync::Arc, time::Duration};

use kandado_app::{
    KandadoApp, KdfParams, Paths, browser::BrowserLink, device_key::MemoryDeviceKey,
    lock::LockReason, secrets::MemorySecrets,
};
use kandado_crypto::{account::AccountBundle, kdf::KdfParams as Params};
use kandado_protocol::AccountKeys;
use kandado_vault::{Store, store::LocalAccount};
use uuid::Uuid;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, keys, user, password, dir] = args.as_slice() else {
        eprintln!("usage: desktop_link <keys.json> <user id> <password> <data dir>");
        std::process::exit(2);
    };
    let keys: AccountKeys =
        serde_json::from_slice(&std::fs::read(keys).expect("keys file")).expect("keys JSON");
    let user_id: Uuid = user.parse().expect("user id");
    let data_dir = std::path::PathBuf::from(dir);
    let _ = std::fs::remove_dir_all(&data_dir);
    let paths = Paths {
        data_dir: data_dir.clone(),
    };
    Store::open(&paths.vault_db())
        .expect("vault file")
        .save_account(&LocalAccount {
            user_id,
            device_id: Uuid::new_v4(),
            email: "juan@example.com".into(),
            display_name: "Juan".into(),
            bundle: AccountBundle {
                version: keys.version,
                kdf_salt: keys.kdf_salt.as_bytes().try_into().expect("salt"),
                kdf_params: Params {
                    memory_kib: keys.kdf.memory_kib,
                    iterations: keys.kdf.iterations,
                    parallelism: keys.kdf.parallelism,
                },
                user_key_by_master: keys.user_key_by_master.as_bytes().to_vec(),
                user_key_by_recovery: keys.user_key_by_recovery.as_bytes().to_vec(),
                public_x25519: keys
                    .public_x25519
                    .as_bytes()
                    .try_into()
                    .expect("public key"),
                private_keys_enc: keys.private_keys_enc.as_bytes().to_vec(),
            },
        })
        .expect("account saved");

    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let app = KandadoApp::with_params(
        paths,
        Arc::new(MemorySecrets::default()),
        Arc::new(MemoryDeviceKey::new()),
        KdfParams::MIN,
        Some("http://127.0.0.1:9"),
    )
    .expect("app");
    runtime.block_on(async {
        app.restore().await.expect("restore");
        app.unlock_with_password(password.clone())
            .await
            .expect("unlock");
    });
    let link = BrowserLink::new(app.clone(), runtime.handle().clone(), data_dir);
    link.set_enabled(true).expect("the browser link starts");
    println!("ready");

    // Accept every pairing, and say its code.
    let accepting = link.clone();
    runtime.spawn(async move {
        loop {
            if let Some(request) = accepting.pending() {
                println!("code {}", request.code);
                let _ = accepting.answer(request.id, true);
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    });

    for line in std::io::stdin().lock().lines() {
        match line.as_deref().map(str::trim) {
            Ok("lock") => {
                app.lock(LockReason::Manual);
                println!("locked");
            }
            Ok("unlock") => {
                runtime
                    .block_on(app.unlock_with_password(password.clone()))
                    .expect("unlock");
                println!("unlocked");
            }
            _ => break,
        }
    }
    link.set_enabled(false)
        .expect("the registration is removed");
    println!("bye");
}
