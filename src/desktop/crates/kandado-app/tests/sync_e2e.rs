//! Phase 3's exit criteria, end to end, against the real `kandado-server` and
//! PostgreSQL, with real sign-in, real keys and the real scheduler:
//!
//! - an edit on one computer shows on the other within two seconds;
//! - edits made offline on both computers merge without loss;
//! - the server can go away and come back with nothing lost.
//!
//! (The property test of random concurrent edits is `kandado-vault`'s
//! `tests/sync.rs`; it needs no server.)
//!
//! ```text
//! docker compose -f src/backend/docker-compose.yml up -d
//! cargo build -p kandado-server
//! DATABASE_URL=postgres://kandado:kandado@127.0.0.1:54329/kandado \
//!   cargo test -p kandado-app --test sync_e2e -- --ignored
//! ```

mod common;

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use common::{Server, computer, google, start_server, start_server_on, unique_email};
use kandado_app::{
    Filter, KandadoApp, Phase,
    sync::{SyncFailure, SyncState},
    tasks::Tasks,
};
use kandado_model::{ItemContent, ItemData, item::Login};
use uuid::Uuid;

const PASSWORD: &str = "correct horse battery staple";

fn login(title: &str, password: &str) -> ItemContent {
    ItemContent::new(
        title,
        ItemData::Login(Login {
            username: "juan".into(),
            password: password.into(),
            ..Login::default()
        }),
    )
}

/// A computer that has created the account and is unlocked.
async fn first_computer(server: &Server, email: &str) -> Arc<KandadoApp> {
    let a = computer("a", server);
    a.restore().await.unwrap();
    let phase = google(&a, server, email).await.unwrap();
    assert!(matches!(phase, Phase::NeedsMasterPassword { .. }));
    a.create_master_password(PASSWORD.into()).await.unwrap();
    assert!(matches!(
        a.confirm_recovery_saved().await.unwrap(),
        Phase::Unlocked { .. }
    ));
    a
}

/// A second computer signed in to the same account and unlocked.
async fn second_computer(server: &Server, email: &str) -> Arc<KandadoApp> {
    let b = computer("b", server);
    b.restore().await.unwrap();
    let phase = google(&b, server, email).await.unwrap();
    assert!(matches!(phase, Phase::Locked { .. }));
    b.unlock_with_password(PASSWORD.into()).await.unwrap();
    b
}

async fn until<T>(what: &str, within: Duration, mut check: impl FnMut() -> Option<T>) -> T {
    let start = Instant::now();
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(start.elapsed() < within, "timed out waiting for {what}");
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

fn password_field(app: &KandadoApp, id: Uuid) -> String {
    let detail = app.item_detail(id).unwrap();
    app.reveal(id, None, detail.password_field.unwrap())
        .unwrap()
        .as_str()
        .to_owned()
}

fn first_vault(app: &KandadoApp) -> Uuid {
    app.vaults().unwrap()[0].id
}

fn titles(app: &KandadoApp) -> Vec<String> {
    let mut titles: Vec<String> = app
        .entries(Filter::default())
        .into_iter()
        .map(|entry| entry.title)
        .collect();
    titles.sort();
    titles
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the server binary and PostgreSQL; see the module docs"]
async fn an_edit_on_one_computer_shows_on_the_other_within_two_seconds() {
    let server = start_server().await;
    let email = unique_email();
    let tasks = Tasks::new().unwrap();

    let a = first_computer(&server, &email).await;
    a.start_sync(&tasks);
    let id = a
        .create_item(first_vault(&a), &login("GitHub", "first"))
        .unwrap();

    // The second computer joins and receives the vault and the item.
    let b = second_computer(&server, &email).await;
    b.start_sync(&tasks);
    until(
        "the item to reach the second computer",
        Duration::from_secs(15),
        || b.entry(id),
    )
    .await;
    assert_eq!(password_field(&b, id), "first");
    // The second computer made no vault of its own.
    assert_eq!(b.vaults().unwrap().len(), 1);
    assert_eq!(b.vaults().unwrap()[0].id, first_vault(&a));

    // Edits both ways, each timed from the edit to the other side showing it.
    for (round, (from, to)) in [(&a, &b), (&b, &a), (&a, &b), (&b, &a)]
        .into_iter()
        .enumerate()
    {
        let value = format!("edit {round}");
        let item = from.item(id).unwrap();
        let mut content = item.content;
        if let ItemData::Login(login) = &mut content.data {
            login.password = value.clone().into();
        }
        let started = Instant::now();
        from.update_item(id, item.vault_id, &content).unwrap();
        until(
            "the edit to reach the other computer",
            Duration::from_secs(10),
            || (password_field(to, id) == value).then_some(()),
        )
        .await;
        let took = started.elapsed();
        eprintln!("round {round}: {took:?}");
        assert!(
            took < Duration::from_secs(2),
            "round {round} took {took:?}, the limit is 2 s"
        );
    }

    // Creating and trashing travel as quickly.
    let started = Instant::now();
    let second = a
        .create_item(first_vault(&a), &login("Second", "pw"))
        .unwrap();
    until("the new item", Duration::from_secs(10), || b.entry(second)).await;
    a.trash_items(&[second]).unwrap();
    until("the trash", Duration::from_secs(10), || {
        b.entry(second).filter(|entry| entry.is_trashed())
    })
    .await;
    assert!(started.elapsed() < Duration::from_secs(4));
    until("syncing to settle", Duration::from_secs(10), || {
        (a.sync_status().state == SyncState::Idle).then_some(())
    })
    .await;
    drop(server);
    // The background runtime cannot be dropped from inside async code.
    tokio::task::spawn_blocking(move || drop(tasks))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the server binary and PostgreSQL; see the module docs"]
async fn offline_edits_on_both_computers_merge_without_loss() {
    let server = start_server().await;
    let email = unique_email();
    // No scheduler here: "offline" is simply not syncing until told to.
    let a = first_computer(&server, &email).await;
    let vault = first_vault(&a);
    let shared = a.create_item(vault, &login("Shared", "start")).unwrap();
    let only_a = a.create_item(vault, &login("Only A", "pw")).unwrap();
    a.sync_now().await.unwrap();
    let b = second_computer(&server, &email).await;
    b.sync_now().await.unwrap();
    assert_eq!(titles(&a), titles(&b));

    // Both computers change things while apart.
    let item = a.item(shared).unwrap();
    let mut a_edit = item.content.clone();
    if let ItemData::Login(login) = &mut a_edit.data {
        login.password = "from a".into();
    }
    a.update_item(shared, vault, &a_edit).unwrap();
    let new_on_a = a.create_item(vault, &login("New on A", "pw")).unwrap();
    a.trash_items(&[only_a]).unwrap();

    let mut b_edit = b.item(shared).unwrap().content;
    b_edit.notes = "note from b".into();
    b.update_item(shared, vault, &b_edit).unwrap();
    let new_on_b = b.create_item(vault, &login("New on B", "pw")).unwrap();
    b.set_favourite(&[only_a], true).unwrap();

    // Back together.
    for app in [&a, &b, &a, &b] {
        app.sync_now().await.unwrap();
    }
    assert_eq!(titles(&a), titles(&b));
    // "Only A" is in the trash, which the list does not show.
    assert_eq!(titles(&a), ["New on A", "New on B", "Shared"]);
    for app in [&a, &b] {
        assert!(app.entry(new_on_a).is_some() && app.entry(new_on_b).is_some());
        // Trashed on one, starred on the other: both stand.
        let merged = app.entry(only_a).unwrap();
        assert!(merged.is_trashed() && merged.favourite);
        // Edited on both: the later writer's version is current...
        assert_eq!(password_field(app, shared), password_field(&a, shared));
    }
    let kept = a.item(shared).unwrap().content;
    let (mine, theirs) = (password_field(&a, shared), kept.notes.clone());
    assert!(
        (mine == "from a" && theirs.is_empty()) || (mine == "start" && theirs == "note from b"),
        "the item is one writer's version: {mine} / {theirs}"
    );
    assert!(!a.sync_status().last_synced.is_none());

    // ...and the other is in the history, from the server.
    for app in [&a, &b] {
        app.fetch_history(shared).await.unwrap();
        let versions = app.history(shared).unwrap();
        assert!(versions.len() >= 3, "start, a's and b's edit: {versions:?}");
    }
    drop(server);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the server binary and PostgreSQL; see the module docs"]
async fn the_server_going_away_and_coming_back_loses_nothing() {
    let server = start_server().await;
    let email = unique_email();
    let a = first_computer(&server, &email).await;
    let vault = first_vault(&a);
    let id = a.create_item(vault, &login("Before", "pw")).unwrap();
    a.sync_now().await.unwrap();
    let b = second_computer(&server, &email).await;
    b.sync_now().await.unwrap();

    // The server stops. Work carries on, and syncing says it is offline.
    let port = server.stop();
    let offline = a.create_item(vault, &login("While offline", "pw")).unwrap();
    a.trash_items(&[id]).unwrap();
    assert!(a.sync_now().await.is_err());
    assert_eq!(a.sync_status().state, SyncState::Offline);
    assert!(a.entry(offline).is_some(), "the vault still works");

    // It comes back (same address, same database); the next sync sends it all.
    let server = start_server_on(port).await;
    a.sync_now().await.unwrap();
    assert_eq!(a.sync_status().state, SyncState::Idle);
    b.sync_now().await.unwrap();
    assert!(b.entry(offline).is_some());
    assert!(b.entry(id).unwrap().is_trashed());
    assert_eq!(titles(&a), titles(&b));
    let _ = SyncFailure::Other;
    drop(server);
}
