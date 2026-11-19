//! Sync against a real PostgreSQL: ordering, conflicts, history, isolation
//! between accounts, tombstones, the trash purge and the change notification.
//!
//! The server never reads item content, so these tests send arbitrary bytes
//! where a client would send ciphertext.

use axum::http::StatusCode;
use kandado_protocol::{
    ChangeResult, ItemKind, ItemVersion, ItemVersions, MAX_BATCH, MAX_BLOB, Opaque, ProblemCode,
    PullResponse, PushRequest, PushResponse, SyncItem, SyncVault,
};
use sqlx::{PgPool, postgres::PgListener};
use uuid::Uuid;

use super::flow_tests::Api;
use crate::db::sync;

fn vault(id: u128, revision: i64) -> SyncVault {
    SyncVault {
        id: Uuid::from_u128(id),
        key_sealed: Opaque::new(vec![1, 2, 3]),
        meta_enc: Opaque::new(vec![9; 16]),
        deleted: false,
        revision,
    }
}

fn item(id: u128, vault: u128, revision: i64) -> SyncItem {
    SyncItem {
        id: Uuid::from_u128(id),
        vault_id: Uuid::from_u128(vault),
        kind: ItemKind::Login,
        key_wrapped: Opaque::new(vec![4; 8]),
        content_enc: Opaque::new(vec![5; 32]),
        version_id: Uuid::from_u128(id * 1000 + revision as u128),
        created_at: 100,
        updated_at: 100 + revision,
        trashed_at: None,
        deleted: false,
        revision,
    }
}

struct Device {
    token: String,
}

async fn device(api: &Api, email: &str) -> Device {
    Device {
        token: api.sign_in(email).await.access_token,
    }
}

async fn push(
    api: &Api,
    who: &Device,
    vaults: Vec<SyncVault>,
    items: Vec<SyncItem>,
) -> PushResponse {
    let reply = api
        .post("/v1/sync", Some(&who.token), &PushRequest { vaults, items })
        .await;
    assert_eq!(
        reply.status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&reply.body)
    );
    reply.json()
}

async fn pull(api: &Api, who: &Device, cursor: i64) -> PullResponse {
    let reply = api
        .get(&format!("/v1/sync?cursor={cursor}"), Some(&who.token))
        .await;
    assert_eq!(reply.status, StatusCode::OK);
    reply.json()
}

fn applied<T: std::fmt::Debug>(result: &ChangeResult<T>) -> i64 {
    match result {
        ChangeResult::Applied { revision } => *revision,
        other => panic!("expected applied, got {other:?}"),
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn new_vaults_and_items_are_stored_and_pulled_in_order(db: PgPool) {
    let api = Api::new(db);
    let ana = device(&api, "ana@example.test").await;
    assert_eq!(pull(&api, &ana, 0).await.cursor, 0);

    let response = push(
        &api,
        &ana,
        vec![vault(1, 0)],
        vec![item(10, 1, 0), item(11, 1, 0)],
    )
    .await;
    let revisions = [
        applied(&response.vaults[0]),
        applied(&response.items[0]),
        applied(&response.items[1]),
    ];
    assert!(
        revisions[0] < revisions[1] && revisions[1] < revisions[2],
        "{revisions:?}"
    );
    assert_eq!(response.cursor, revisions[2]);

    let pulled = pull(&api, &ana, 0).await;
    assert_eq!(pulled.vaults.len(), 1);
    assert_eq!(pulled.items.len(), 2);
    assert_eq!(pulled.cursor, revisions[2]);
    assert!(!pulled.more);
    assert_eq!(pulled.vaults[0].key_sealed, Opaque::new(vec![1, 2, 3]));
    assert_eq!(pulled.items[0].content_enc, Opaque::new(vec![5; 32]));
    // Nothing is new after the cursor.
    let again = pull(&api, &ana, pulled.cursor).await;
    assert!(again.items.is_empty() && again.vaults.is_empty());
    assert_eq!(again.cursor, pulled.cursor);
}

#[sqlx::test(migrations = "../migrations")]
async fn a_pull_can_be_paged(db: PgPool) {
    let api = Api::new(db);
    let ana = device(&api, "ana@example.test").await;
    let items: Vec<SyncItem> = (0..5).map(|n| item(10 + n, 1, 0)).collect();
    push(&api, &ana, vec![vault(1, 0)], items).await;

    let (mut cursor, mut seen, mut rounds) = (0, 0, 0);
    loop {
        let reply = api
            .get(
                &format!("/v1/sync?cursor={cursor}&limit=2"),
                Some(&ana.token),
            )
            .await;
        let page: PullResponse = reply.json();
        seen += page.items.len() + page.vaults.len();
        assert!(page.items.len() + page.vaults.len() <= 2);
        cursor = page.cursor;
        rounds += 1;
        if !page.more {
            break;
        }
        assert!(rounds < 10);
    }
    assert_eq!((seen, rounds), (6, 3));
}

#[sqlx::test(migrations = "../migrations")]
async fn a_change_against_an_older_revision_is_a_conflict(db: PgPool) {
    let api = Api::new(db);
    let ana = device(&api, "ana@example.test").await;
    let first = push(&api, &ana, vec![vault(1, 0)], vec![item(10, 1, 0)]).await;
    let base = applied(&first.items[0]);

    // Another device edits first.
    let mut theirs = item(10, 1, base);
    theirs.version_id = Uuid::from_u128(77);
    theirs.content_enc = Opaque::new(vec![7; 8]);
    let second = push(&api, &ana, vec![], vec![theirs.clone()]).await;
    let newer = applied(&second.items[0]);
    assert!(newer > base);

    // This one was made against the old revision.
    let mut mine = item(10, 1, base);
    mine.version_id = Uuid::from_u128(88);
    let response = push(&api, &ana, vec![], vec![mine.clone()]).await;
    match &response.items[0] {
        ChangeResult::Conflict { current } => {
            assert_eq!(current.revision, newer);
            assert_eq!(current.version_id, Uuid::from_u128(77));
            assert_eq!(current.content_enc, Opaque::new(vec![7; 8]));
        }
        other => panic!("expected a conflict, got {other:?}"),
    }
    // Sent again on top of theirs it goes through, and nothing was lost:
    // all three versions are in the history.
    mine.revision = newer;
    let third = push(&api, &ana, vec![], vec![mine]).await;
    assert!(applied(&third.items[0]) > newer);
    let reply = api
        .get(
            &format!("/v1/items/{}/revisions", Uuid::from_u128(10)),
            Some(&ana.token),
        )
        .await;
    let versions: ItemVersions = reply.json();
    let ids: Vec<Uuid> = versions.versions.iter().map(|v| v.version_id).collect();
    assert_eq!(ids.len(), 3);
    for expected in [
        Uuid::from_u128(10_000),
        Uuid::from_u128(77),
        Uuid::from_u128(88),
    ] {
        assert!(ids.contains(&expected), "{expected}");
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn saving_without_new_content_adds_no_version_and_trash_is_not_an_edit(db: PgPool) {
    let api = Api::new(db);
    let ana = device(&api, "ana@example.test").await;
    let first = push(&api, &ana, vec![vault(1, 0)], vec![item(10, 1, 0)]).await;
    let base = applied(&first.items[0]);
    let mut trashed = item(10, 1, base);
    trashed.version_id = Uuid::from_u128(10_000);
    trashed.trashed_at = Some(500);
    let second = push(&api, &ana, vec![], vec![trashed]).await;
    assert!(applied(&second.items[0]) > base);
    let pulled = pull(&api, &ana, base).await;
    assert_eq!(pulled.items[0].trashed_at, Some(500));

    let reply = api
        .get(
            &format!("/v1/items/{}/revisions", Uuid::from_u128(10)),
            Some(&ana.token),
        )
        .await;
    assert_eq!(reply.json::<ItemVersions>().versions.len(), 1);
    let reply = api
        .get(
            &format!(
                "/v1/items/{}/revisions/{}",
                Uuid::from_u128(10),
                Uuid::from_u128(10_000)
            ),
            Some(&ana.token),
        )
        .await;
    assert_eq!(
        reply.json::<ItemVersion>().content_enc,
        Opaque::new(vec![5; 32])
    );
    let missing = api
        .get(
            &format!(
                "/v1/items/{}/revisions/{}",
                Uuid::from_u128(10),
                Uuid::from_u128(5)
            ),
            Some(&ana.token),
        )
        .await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../migrations")]
async fn another_account_sees_and_changes_nothing(db: PgPool) {
    let api = Api::new(db);
    let ana = device(&api, "ana@example.test").await;
    let bob = device(&api, "bob@example.test").await;
    let first = push(&api, &ana, vec![vault(1, 0)], vec![item(10, 1, 0)]).await;
    let base = applied(&first.items[0]);

    // Bob pulls nothing and cannot read the history.
    let pulled = pull(&api, &bob, 0).await;
    assert!(pulled.vaults.is_empty() && pulled.items.is_empty());
    let reply = api
        .get(
            &format!("/v1/items/{}/revisions", Uuid::from_u128(10)),
            Some(&bob.token),
        )
        .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);

    // He cannot add to Ana's vault, overwrite her item, take over her vault
    // id, or tell that any of them exist.
    let response = push(
        &api,
        &bob,
        vec![vault(1, 0), vault(1, 5)],
        vec![item(20, 1, 0), item(10, 1, base), item(10, 2, 0)],
    )
    .await;
    assert!(
        response.vaults.iter().all(|r| *r == ChangeResult::Rejected),
        "{:?}",
        response.vaults
    );
    assert!(
        response.items.iter().all(|r| *r == ChangeResult::Rejected),
        "{:?}",
        response.items
    );

    // Ana's data is untouched.
    let after = pull(&api, &ana, 0).await;
    assert_eq!(after.items.len(), 1);
    assert_eq!(after.items[0].revision, base);
    assert_eq!(after.items[0].content_enc, Opaque::new(vec![5; 32]));

    // Bob can keep his own data under his own vault, and moving an item into
    // a vault he is not in is refused.
    let response = push(&api, &bob, vec![vault(2, 0)], vec![item(30, 2, 0)]).await;
    assert!(matches!(response.items[0], ChangeResult::Applied { .. }));
    let mut moved = item(30, 1, applied(&response.items[0]));
    moved.version_id = Uuid::from_u128(30_000);
    let response = push(&api, &bob, vec![], vec![moved]).await;
    assert_eq!(response.items[0], ChangeResult::Rejected);
}

#[sqlx::test(migrations = "../migrations")]
async fn deleting_for_good_leaves_only_a_tombstone_that_beats_old_edits(db: PgPool) {
    let api = Api::new(db);
    let ana = device(&api, "ana@example.test").await;
    let first = push(&api, &ana, vec![vault(1, 0)], vec![item(10, 1, 0)]).await;
    let base = applied(&first.items[0]);

    let mut gone = item(10, 1, base);
    gone.deleted = true;
    gone.content_enc = Opaque::default();
    let second = push(&api, &ana, vec![], vec![gone]).await;
    let tombstone = applied(&second.items[0]);

    let pulled = pull(&api, &ana, base).await;
    assert_eq!(pulled.items.len(), 1);
    assert!(pulled.items[0].deleted);
    assert!(pulled.items[0].content_enc.is_empty());
    assert_eq!(pulled.items[0].revision, tombstone);
    // The history went with it.
    let reply = api
        .get(
            &format!("/v1/items/{}/revisions", Uuid::from_u128(10)),
            Some(&ana.token),
        )
        .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);

    // A device that never heard of the deletion edits it, even against the
    // tombstone's own revision: the tombstone wins.
    for revision in [base, tombstone] {
        let mut late = item(10, 1, revision);
        late.version_id = Uuid::from_u128(99);
        let response = push(&api, &ana, vec![], vec![late]).await;
        match &response.items[0] {
            ChangeResult::Conflict { current } => assert!(current.deleted),
            other => panic!("expected the tombstone, got {other:?}"),
        }
    }
}

#[sqlx::test(migrations = "../migrations")]
async fn items_in_the_trash_are_purged_after_the_time_and_not_before(db: PgPool) {
    let api = Api::new(db.clone());
    let ana = device(&api, "ana@example.test").await;
    let first = push(
        &api,
        &ana,
        vec![vault(1, 0)],
        vec![item(10, 1, 0), item(11, 1, 0)],
    )
    .await;
    let mut trashed = item(10, 1, applied(&first.items[0]));
    trashed.version_id = Uuid::from_u128(10_000);
    trashed.trashed_at = Some(500);
    let response = push(&api, &ana, vec![], vec![trashed]).await;
    let cursor = applied(&response.items[0]);

    // Not old enough.
    assert_eq!(sync::purge_trash(&db, 30).await.unwrap(), 0);
    assert!(pull(&api, &ana, cursor).await.items.is_empty());

    // Make it 31 days old on the server's clock.
    sqlx::query("UPDATE items SET trashed_on = now() - interval '31 days' WHERE id = $1")
        .bind(Uuid::from_u128(10))
        .execute(&db)
        .await
        .unwrap();
    assert_eq!(sync::purge_trash(&db, 30).await.unwrap(), 1);
    let pulled = pull(&api, &ana, cursor).await;
    assert_eq!(pulled.items.len(), 1);
    assert!(pulled.items[0].deleted && pulled.items[0].id == Uuid::from_u128(10));
    // The other item is untouched, and the purge is idempotent.
    assert_eq!(sync::purge_trash(&db, 30).await.unwrap(), 0);
    let revisions: i64 =
        sqlx::query_scalar("SELECT count(*) FROM item_revisions WHERE item_id = $1")
            .bind(Uuid::from_u128(10))
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(revisions, 0);

    // Taking an item out of the trash stops its clock.
    let mut restored = item(
        11,
        1,
        pull(&api, &ana, 0)
            .await
            .items
            .iter()
            .find(|i| i.id == Uuid::from_u128(11))
            .unwrap()
            .revision,
    );
    restored.version_id = Uuid::from_u128(11_000);
    restored.trashed_at = Some(1);
    let r = push(&api, &ana, vec![], vec![restored.clone()]).await;
    restored.revision = applied(&r.items[0]);
    restored.trashed_at = None;
    push(&api, &ana, vec![], vec![restored]).await;
    let on: Option<time::OffsetDateTime> =
        sqlx::query_scalar("SELECT trashed_on FROM items WHERE id = $1")
            .bind(Uuid::from_u128(11))
            .fetch_one(&db)
            .await
            .unwrap();
    assert!(on.is_none());
}

#[sqlx::test(migrations = "../migrations")]
async fn invalid_changes_are_refused(db: PgPool) {
    let api = Api::new(db);
    let ana = device(&api, "ana@example.test").await;
    let first = push(&api, &ana, vec![vault(1, 0)], vec![item(10, 1, 0)]).await;
    let base = applied(&first.items[0]);

    // A kind cannot change, a new row cannot claim a revision, and a blob
    // cannot be huge.
    let mut other_kind = item(10, 1, base);
    other_kind.kind = ItemKind::Card;
    other_kind.version_id = Uuid::from_u128(1);
    let mut phantom = item(40, 1, 5);
    phantom.version_id = Uuid::from_u128(2);
    let mut huge = item(41, 1, 0);
    huge.content_enc = Opaque::new(vec![0; MAX_BLOB + 1]);
    let response = push(
        &api,
        &ana,
        vec![vault(3, 9)],
        vec![other_kind, phantom, huge],
    )
    .await;
    assert_eq!(response.vaults[0], ChangeResult::Rejected);
    assert!(
        response.items.iter().all(|r| *r == ChangeResult::Rejected),
        "{:?}",
        response.items
    );

    // Too many changes at once.
    let many: Vec<SyncItem> = (0..=MAX_BATCH as u128)
        .map(|n| item(100 + n, 1, 0))
        .collect();
    let reply = api
        .post(
            "/v1/sync",
            Some(&ana.token),
            &PushRequest {
                vaults: vec![],
                items: many,
            },
        )
        .await;
    assert_eq!(reply.status, StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(reply.problem().code, ProblemCode::PayloadTooLarge);

    // A bad cursor, and no token.
    let reply = api.get("/v1/sync?cursor=-1", Some(&ana.token)).await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);
    let reply = api.get("/v1/sync", None).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "../migrations")]
async fn a_large_batch_is_accepted_and_a_vault_can_be_renamed(db: PgPool) {
    let api = Api::new(db);
    let ana = device(&api, "ana@example.test").await;
    // Well over the 64 KiB limit of other routes.
    let items: Vec<SyncItem> = (0..60)
        .map(|n| {
            let mut item = item(10 + n, 1, 0);
            item.content_enc = Opaque::new(vec![1; 8 * 1024]);
            item
        })
        .collect();
    let response = push(&api, &ana, vec![vault(1, 0)], items).await;
    assert!(
        response
            .items
            .iter()
            .all(|r| matches!(r, ChangeResult::Applied { .. }))
    );

    let base = applied(&response.vaults[0]);
    let mut renamed = vault(1, base);
    renamed.meta_enc = Opaque::new(vec![8; 20]);
    let response = push(&api, &ana, vec![renamed.clone()], vec![]).await;
    let new_revision = applied(&response.vaults[0]);
    // The same change again is now stale.
    let response = push(&api, &ana, vec![renamed], vec![]).await;
    assert!(matches!(response.vaults[0], ChangeResult::Conflict { .. }));
    let pulled = pull(&api, &ana, base).await;
    assert_eq!(pulled.vaults[0].meta_enc, Opaque::new(vec![8; 20]));
    assert_eq!(pulled.vaults[0].revision, new_revision);
}

#[sqlx::test(migrations = "../migrations")]
async fn a_write_notifies_its_account_only_after_it_commits(db: PgPool) {
    let api = Api::new(db.clone());
    let ana = device(&api, "ana@example.test").await;
    let bob = device(&api, "bob@example.test").await;
    let ana_id = api.sign_in("ana@example.test").await.user_id;

    let mut listener = PgListener::connect_with(&db).await.unwrap();
    listener.listen(sync::CHANNEL).await.unwrap();

    push(&api, &ana, vec![vault(1, 0)], vec![item(10, 1, 0)]).await;
    let notification = tokio::time::timeout(std::time::Duration::from_secs(5), listener.recv())
        .await
        .expect("a notification arrives")
        .unwrap();
    assert_eq!(notification.payload(), ana_id.to_string());

    // A refused change does not notify.
    push(&api, &bob, vec![vault(1, 0)], vec![item(10, 1, 0)]).await;
    let nothing =
        tokio::time::timeout(std::time::Duration::from_millis(300), listener.recv()).await;
    assert!(nothing.is_err(), "no notification for a rejected change");
}

#[sqlx::test(migrations = "../migrations")]
async fn concurrent_writers_of_one_account_never_share_or_reorder_revisions(db: PgPool) {
    let api = Api::new(db);
    let ana = device(&api, "ana@example.test").await;
    push(&api, &ana, vec![vault(1, 0)], vec![]).await;

    let mut tasks = Vec::new();
    for n in 0..8u128 {
        let (api, token) = (Api::new(api.db.clone()), ana.token.clone());
        tasks.push(tokio::spawn(async move {
            let who = Device { token };
            let items: Vec<SyncItem> = (0..5).map(|m| item(1_000 + n * 10 + m, 1, 0)).collect();
            push(&api, &who, vec![], items).await
        }));
    }
    let mut revisions = Vec::new();
    for task in tasks {
        for result in task.await.unwrap().items {
            revisions.push(applied(&result));
        }
    }
    let mut sorted = revisions.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 40, "every revision is unique");

    // Paging from the start sees every item exactly once.
    let (mut cursor, mut seen) = (0, Vec::new());
    loop {
        let reply = api
            .get(
                &format!("/v1/sync?cursor={cursor}&limit=7"),
                Some(&ana.token),
            )
            .await;
        let page: PullResponse = reply.json();
        seen.extend(page.items.iter().map(|i| i.id));
        cursor = page.cursor;
        if !page.more {
            break;
        }
    }
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen.len(), 40);
}

fn deletion(id: u128) -> SyncVault {
    SyncVault {
        key_sealed: Opaque::default(),
        meta_enc: Opaque::default(),
        deleted: true,
        ..vault(id, 0)
    }
}

fn is_deleted_vault(result: &ChangeResult<SyncVault>) -> bool {
    matches!(result, ChangeResult::Conflict { current } if current.deleted)
}

fn is_deleted_item(result: &ChangeResult<SyncItem>) -> bool {
    matches!(result, ChangeResult::Conflict { current } if current.deleted)
}

#[sqlx::test(migrations = "../migrations")]
async fn deleting_a_vault_reaches_every_device_with_its_items(db: PgPool) {
    let api = Api::new(db.clone());
    let laptop = device(&api, "ana@example.test").await;
    let desktop = device(&api, "ana@example.test").await;
    let ana_id = api.sign_in("ana@example.test").await.user_id;
    let mut trashed = item(11, 1, 0);
    trashed.trashed_at = Some(150);
    let first = push(
        &api,
        &laptop,
        vec![vault(1, 0), vault(2, 0)],
        vec![item(10, 1, 0), trashed, item(12, 2, 0)],
    )
    .await;
    let kept = applied(&first.items[2]);
    let seen = pull(&api, &desktop, 0).await.cursor;

    let mut listener = PgListener::connect_with(&db).await.unwrap();
    listener.listen(sync::CHANNEL).await.unwrap();
    let response = push(&api, &laptop, vec![deletion(1)], vec![]).await;
    let revision = applied(&response.vaults[0]);
    assert_eq!(response.cursor, revision, "the vault is deleted last");
    let notification = tokio::time::timeout(std::time::Duration::from_secs(5), listener.recv())
        .await
        .expect("the other devices are told")
        .unwrap();
    assert_eq!(notification.payload(), ana_id.to_string());

    // The other device learns of the vault and of each of its items, the
    // trashed one too; the other vault's item is untouched.
    let pulled = pull(&api, &desktop, seen).await;
    assert_eq!(pulled.vaults.len(), 1);
    assert!(pulled.vaults[0].deleted);
    assert_eq!(pulled.vaults[0].revision, revision);
    let mut gone: Vec<_> = pulled.items.iter().map(|item| item.id).collect();
    gone.sort();
    assert_eq!(gone, [Uuid::from_u128(10), Uuid::from_u128(11)]);
    assert!(pulled.items.iter().all(|item| item.deleted
        && item.content_enc.is_empty()
        && item.key_wrapped.is_empty()
        && item.revision < revision));
    // So does a device that starts from nothing.
    let fresh = pull(&api, &desktop, 0).await;
    let vault_one = fresh.vaults.iter().find(|v| v.id == Uuid::from_u128(1));
    assert!(vault_one.is_some_and(|vault| vault.deleted));
    let other = fresh.items.iter().find(|i| i.id == Uuid::from_u128(12));
    assert!(other.is_some_and(|item| !item.deleted && item.revision == kept));
    // The history went with the items.
    let reply = api
        .get(
            &format!("/v1/items/{}/revisions", Uuid::from_u128(10)),
            Some(&desktop.token),
        )
        .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    let history: i64 =
        sqlx::query_scalar("SELECT count(*) FROM item_revisions WHERE item_id = ANY($1)")
            .bind([Uuid::from_u128(10), Uuid::from_u128(11)])
            .fetch_one(&db)
            .await
            .unwrap();
    assert_eq!(history, 0);

    // A device that has not heard of it yet renames the vault, edits an item,
    // moves one out to the other vault, deletes the vault again: each is
    // answered with the deletion. A new item in it is refused.
    let mut renamed = vault(1, first.cursor);
    renamed.meta_enc = Opaque::new(vec![7; 16]);
    let mut edited = item(10, 1, applied(&first.items[0]));
    edited.version_id = Uuid::from_u128(77);
    let mut moved = item(11, 2, applied(&first.items[1]));
    moved.version_id = Uuid::from_u128(78);
    let late = push(
        &api,
        &desktop,
        vec![renamed, deletion(1)],
        vec![edited, moved, item(13, 1, 0)],
    )
    .await;
    assert!(
        late.vaults.iter().all(is_deleted_vault),
        "{:?}",
        late.vaults
    );
    assert!(is_deleted_item(&late.items[0]), "{:?}", late.items[0]);
    assert!(is_deleted_item(&late.items[1]), "{:?}", late.items[1]);
    assert_eq!(late.items[2], ChangeResult::Rejected);
    assert_eq!(late.cursor, revision, "nothing was written");
}

#[sqlx::test(migrations = "../migrations")]
async fn only_the_owner_deletes_a_vault_and_never_their_last(db: PgPool) {
    let api = Api::new(db);
    let ana = device(&api, "ana@example.test").await;
    let bob = device(&api, "bob@example.test").await;
    let first = push(
        &api,
        &ana,
        vec![vault(1, 0), vault(2, 0)],
        vec![item(10, 1, 0)],
    )
    .await;
    push(&api, &bob, vec![vault(3, 0)], vec![]).await;

    // Bob cannot delete Ana's vault, at any revision, nor tell it exists.
    let mut at_revision = deletion(1);
    at_revision.revision = applied(&first.vaults[0]);
    let refused = push(&api, &bob, vec![deletion(1), at_revision], vec![]).await;
    assert!(
        refused.vaults.iter().all(|r| *r == ChangeResult::Rejected),
        "{:?}",
        refused.vaults
    );
    let after = pull(&api, &ana, 0).await;
    assert!(after.vaults.iter().all(|vault| !vault.deleted));
    assert_eq!(after.items.len(), 1);
    assert!(!after.items[0].deleted);
    assert_eq!(after.items[0].content_enc, Opaque::new(vec![5; 32]));

    // A vault the server never had has nothing to delete.
    let unknown = push(&api, &ana, vec![deletion(9)], vec![]).await;
    assert_eq!(unknown.vaults[0], ChangeResult::Rejected);

    // Ana deletes one of her two, but not the one left.
    let deleted = push(&api, &ana, vec![deletion(1)], vec![]).await;
    applied(&deleted.vaults[0]);
    let last = push(&api, &ana, vec![deletion(2)], vec![]).await;
    assert_eq!(last.vaults[0], ChangeResult::Rejected);
    let after = pull(&api, &ana, 0).await;
    let two = after.vaults.iter().find(|v| v.id == Uuid::from_u128(2));
    assert!(two.is_some_and(|vault| !vault.deleted));
    // Bob's own vault is untouched by any of it.
    let bobs = pull(&api, &bob, 0).await;
    assert_eq!(bobs.vaults.len(), 1);
    assert!(!bobs.vaults[0].deleted);
}
