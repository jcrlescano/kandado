//! Secure links against a real PostgreSQL: who may make one, view limits,
//! expiry, revoking, what happens when the item is deleted, and that the
//! payload is gone from the database as soon as a link stops working.
//!
//! The server never reads the payload, so these tests send arbitrary bytes.

use axum::http::{Method, StatusCode};
use kandado_protocol::{
    ChangeResult, CreateLink, ItemKind, LinkCreated, LinkList, LinkState, MAX_LINK_PAYLOAD, Opaque,
    OpenedLink, ProblemCode, PushRequest, PushResponse, SyncItem, SyncVault,
};
use sqlx::PgPool;
use uuid::Uuid;

use super::flow_tests::Api;
use crate::db::links;

const DAY: i64 = 86_400;

fn vault(id: u128) -> SyncVault {
    SyncVault {
        id: Uuid::from_u128(id),
        key_sealed: Opaque::new(vec![1, 2, 3]),
        meta_enc: Opaque::new(vec![9; 16]),
        deleted: false,
        revision: 0,
    }
}

fn item(id: u128, vault: u128, kind: ItemKind) -> SyncItem {
    SyncItem {
        id: Uuid::from_u128(id),
        vault_id: Uuid::from_u128(vault),
        kind,
        key_wrapped: Opaque::new(vec![4; 8]),
        content_enc: Opaque::new(vec![5; 32]),
        version_id: Uuid::from_u128(id * 1000),
        created_at: 100,
        updated_at: 100,
        trashed_at: None,
        deleted: false,
        revision: 0,
    }
}

/// Signs in and stores a vault with a login (id 10) and a passkey (id 11).
async fn account(api: &Api, email: &str, vault_id: u128) -> String {
    let token = api.sign_in(email).await.access_token;
    let base = vault_id * 100;
    let reply = api
        .post(
            "/v1/sync",
            Some(&token),
            &PushRequest {
                vaults: vec![vault(vault_id)],
                items: vec![
                    item(base + 10, vault_id, ItemKind::Login),
                    item(base + 11, vault_id, ItemKind::Passkey),
                ],
            },
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK);
    token
}

fn request(id: u128, item_id: u128, max_views: Option<i32>) -> CreateLink {
    CreateLink {
        id: Uuid::from_u128(id),
        item_id: Uuid::from_u128(item_id),
        payload_enc: Opaque::new(vec![7; 64]),
        expires_in: DAY,
        max_views,
    }
}

async fn open(api: &Api, id: u128) -> (StatusCode, Option<OpenedLink>) {
    let reply = api
        .send(
            Method::POST,
            &format!("/v1/public/links/{}/open", Uuid::from_u128(id)),
            None,
            None,
        )
        .await;
    let opened = (reply.status == StatusCode::OK).then(|| reply.json());
    (reply.status, opened)
}

async fn list(api: &Api, token: &str) -> LinkList {
    let reply = api.get("/v1/links", Some(token)).await;
    assert_eq!(reply.status, StatusCode::OK);
    reply.json()
}

async fn stored_payload(db: &PgPool, id: u128) -> Option<Vec<u8>> {
    sqlx::query_scalar("SELECT payload_enc FROM secure_links WHERE id = $1")
        .bind(Uuid::from_u128(id))
        .fetch_one(db)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../migrations")]
async fn a_link_opens_until_its_views_run_out_then_forgets_the_payload(db: PgPool) {
    let api = Api::new(db.clone());
    let ana = account(&api, "ana@example.test", 1).await;

    let reply = api
        .post("/v1/links", Some(&ana), &request(1, 110, Some(5)))
        .await;
    assert_eq!(reply.status, StatusCode::CREATED);
    let created: LinkCreated = reply.json();
    assert_eq!(
        created.url,
        format!("http://127.0.0.1:8080/l/{}", Uuid::from_u128(1))
    );

    // Anyone can open it, without signing in.
    let (status, first) = open(&api, 1).await;
    assert_eq!(status, StatusCode::OK);
    let first = first.unwrap();
    assert_eq!(first.payload_enc, Opaque::new(vec![7; 64]));
    assert_eq!(first.views_left, Some(4));
    assert_eq!(first.expires_at, created.expires_at);
    for left in [3, 2, 1] {
        assert!(stored_payload(&db, 1).await.is_some());
        assert_eq!(open(&api, 1).await.1.unwrap().views_left, Some(left));
    }

    let (_, last) = open(&api, 1).await;
    assert_eq!(last.unwrap().views_left, Some(0));
    // The last view takes the payload with it.
    assert_eq!(stored_payload(&db, 1).await, None);
    let (status, _) = open(&api, 1).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let listed = list(&api, &ana).await;
    assert_eq!(listed.links.len(), 1);
    let link = &listed.links[0];
    assert_eq!(link.item_id, Uuid::from_u128(110));
    assert_eq!((link.views, link.max_views), (5, Some(5)));
    assert_eq!(link.state, LinkState::UsedUp);
}

#[sqlx::test(migrations = "../migrations")]
async fn a_link_with_no_limit_keeps_counting(db: PgPool) {
    let api = Api::new(db);
    let ana = account(&api, "ana@example.test", 1).await;
    api.post("/v1/links", Some(&ana), &request(1, 110, None))
        .await;
    for _ in 0..3 {
        let (status, opened) = open(&api, 1).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(opened.unwrap().views_left, None);
    }
    let link = &list(&api, &ana).await.links[0];
    assert_eq!((link.views, link.state), (3, LinkState::Active));
}

#[sqlx::test(migrations = "../migrations")]
async fn revoking_stops_a_link_at_once_and_only_its_owner_can(db: PgPool) {
    let api = Api::new(db.clone());
    let ana = account(&api, "ana@example.test", 1).await;
    let ben = account(&api, "ben@example.test", 2).await;
    api.post("/v1/links", Some(&ana), &request(1, 110, Some(5)))
        .await;
    let path = format!("/v1/links/{}", Uuid::from_u128(1));

    // Someone else's link looks like no link.
    let reply = api.send(Method::DELETE, &path, Some(&ben), None).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert!(list(&api, &ben).await.links.is_empty());
    assert_eq!(open(&api, 1).await.0, StatusCode::OK);

    let reply = api.send(Method::DELETE, &path, Some(&ana), None).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    assert_eq!(stored_payload(&db, 1).await, None);
    assert_eq!(open(&api, 1).await.0, StatusCode::NOT_FOUND);
    assert_eq!(list(&api, &ana).await.links[0].state, LinkState::Revoked);
    // Again is fine.
    let reply = api.send(Method::DELETE, &path, Some(&ana), None).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
}

#[sqlx::test(migrations = "../migrations")]
async fn only_your_own_shareable_items_can_be_linked(db: PgPool) {
    let api = Api::new(db);
    let ana = account(&api, "ana@example.test", 1).await;
    let ben = account(&api, "ben@example.test", 2).await;

    let check = async |token: &str, link: CreateLink, status: StatusCode, code: ProblemCode| {
        let reply = api.post("/v1/links", Some(token), &link).await;
        assert_eq!(
            reply.status,
            status,
            "{}",
            String::from_utf8_lossy(&reply.body)
        );
        assert_eq!(reply.problem().code, code);
    };
    // Ben's login, an item that does not exist, Ana's own passkey.
    check(
        &ana,
        request(1, 210, None),
        StatusCode::NOT_FOUND,
        ProblemCode::NotFound,
    )
    .await;
    check(
        &ana,
        request(2, 999, None),
        StatusCode::NOT_FOUND,
        ProblemCode::NotFound,
    )
    .await;
    check(
        &ana,
        request(3, 111, None),
        StatusCode::NOT_FOUND,
        ProblemCode::NotFound,
    )
    .await;

    let mut odd = request(4, 110, None);
    odd.expires_in = 12_345;
    check(&ana, odd, StatusCode::BAD_REQUEST, ProblemCode::BadRequest).await;
    check(
        &ana,
        request(5, 110, Some(3)),
        StatusCode::BAD_REQUEST,
        ProblemCode::BadRequest,
    )
    .await;
    let mut empty = request(6, 110, None);
    empty.payload_enc = Opaque::new(Vec::new());
    check(
        &ana,
        empty,
        StatusCode::BAD_REQUEST,
        ProblemCode::BadRequest,
    )
    .await;
    let mut large = request(7, 110, None);
    large.payload_enc = Opaque::new(vec![0; MAX_LINK_PAYLOAD + 1]);
    check(
        &ana,
        large,
        StatusCode::PAYLOAD_TOO_LARGE,
        ProblemCode::PayloadTooLarge,
    )
    .await;

    let reply = api
        .post("/v1/links", Some(&ana), &request(8, 110, None))
        .await;
    assert_eq!(reply.status, StatusCode::CREATED);
    // Ben cannot reuse the id, not even for his own item.
    check(
        &ben,
        request(8, 210, None),
        StatusCode::CONFLICT,
        ProblemCode::Conflict,
    )
    .await;
    // Signed out, nothing.
    let reply = api.post("/v1/links", None, &request(9, 110, None)).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        api.get("/v1/links", None).await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test(migrations = "../migrations")]
async fn an_expired_link_stops_and_the_job_clears_it_then_deletes_it(db: PgPool) {
    let api = Api::new(db.clone());
    let ana = account(&api, "ana@example.test", 1).await;
    api.post("/v1/links", Some(&ana), &request(1, 110, None))
        .await;
    api.post("/v1/links", Some(&ana), &request(2, 110, None))
        .await;

    links::age(&db, Uuid::from_u128(1), 2).await.unwrap();
    assert_eq!(open(&api, 1).await.0, StatusCode::NOT_FOUND);
    assert_eq!(open(&api, 2).await.0, StatusCode::OK);
    let states: Vec<_> = list(&api, &ana)
        .await
        .links
        .iter()
        .map(|link| (link.id, link.state))
        .collect();
    assert!(states.contains(&(Uuid::from_u128(1), LinkState::Expired)));
    assert!(states.contains(&(Uuid::from_u128(2), LinkState::Active)));

    // The job drops the expired payload and leaves the working one alone.
    assert!(stored_payload(&db, 1).await.is_some());
    assert_eq!(links::expire(&db).await.unwrap(), (1, 0));
    assert_eq!(stored_payload(&db, 1).await, None);
    assert!(stored_payload(&db, 2).await.is_some());

    // Thirty days after it ended, it leaves the list.
    links::age(&db, Uuid::from_u128(1), 30).await.unwrap();
    assert_eq!(links::expire(&db).await.unwrap(), (0, 1));
    let left = list(&api, &ana).await.links;
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, Uuid::from_u128(2));
}

#[sqlx::test(migrations = "../migrations")]
async fn deleting_the_item_for_good_ends_its_links(db: PgPool) {
    let api = Api::new(db.clone());
    let ana = account(&api, "ana@example.test", 1).await;
    api.post("/v1/links", Some(&ana), &request(1, 110, None))
        .await;

    // In the trash, the link still works: the item can come back.
    let pulled: kandado_protocol::PullResponse =
        api.get("/v1/sync?cursor=0", Some(&ana)).await.json();
    let mut current = pulled
        .items
        .into_iter()
        .find(|item| item.id == Uuid::from_u128(110))
        .unwrap();
    current.trashed_at = Some(200);
    let pushed: PushResponse = api
        .post(
            "/v1/sync",
            Some(&ana),
            &PushRequest {
                vaults: vec![],
                items: vec![current.clone()],
            },
        )
        .await
        .json();
    let ChangeResult::Applied { revision } = pushed.items[0] else {
        panic!("{:?}", pushed.items[0]);
    };
    assert_eq!(open(&api, 1).await.0, StatusCode::OK);

    current.revision = revision;
    current.deleted = true;
    let pushed: PushResponse = api
        .post(
            "/v1/sync",
            Some(&ana),
            &PushRequest {
                vaults: vec![],
                items: vec![current],
            },
        )
        .await
        .json();
    assert!(matches!(pushed.items[0], ChangeResult::Applied { .. }));
    assert_eq!(open(&api, 1).await.0, StatusCode::NOT_FOUND);
    assert_eq!(stored_payload(&db, 1).await, None);
    assert_eq!(list(&api, &ana).await.links[0].state, LinkState::Revoked);
}

#[sqlx::test(migrations = "../migrations")]
async fn deleting_the_vault_ends_the_links_to_its_items(db: PgPool) {
    let api = Api::new(db.clone());
    let ana = account(&api, "ana@example.test", 1).await;
    // A second vault, so the first is not her last.
    let reply = api
        .post(
            "/v1/sync",
            Some(&ana),
            &PushRequest {
                vaults: vec![vault(2)],
                items: vec![],
            },
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK);
    api.post("/v1/links", Some(&ana), &request(1, 110, None))
        .await;
    assert_eq!(open(&api, 1).await.0, StatusCode::OK);

    let pushed: PushResponse = api
        .post(
            "/v1/sync",
            Some(&ana),
            &PushRequest {
                vaults: vec![SyncVault {
                    deleted: true,
                    ..vault(1)
                }],
                items: vec![],
            },
        )
        .await
        .json();
    assert!(matches!(pushed.vaults[0], ChangeResult::Applied { .. }));
    assert_eq!(open(&api, 1).await.0, StatusCode::NOT_FOUND);
    assert_eq!(stored_payload(&db, 1).await, None);
    assert_eq!(list(&api, &ana).await.links[0].state, LinkState::Revoked);
}

#[sqlx::test(migrations = "../migrations")]
async fn concurrent_opens_never_exceed_the_limit(db: PgPool) {
    let api = std::sync::Arc::new(Api::new(db));
    let ana = account(&api, "ana@example.test", 1).await;
    api.post("/v1/links", Some(&ana), &request(1, 110, Some(5)))
        .await;
    let tasks: Vec<_> = (0..20)
        .map(|_| {
            let api = api.clone();
            tokio::spawn(async move { open(&api, 1).await.0 })
        })
        .collect();
    let mut opened = 0;
    for task in tasks {
        if task.await.unwrap() == StatusCode::OK {
            opened += 1;
        }
    }
    assert_eq!(opened, 5);
    assert_eq!(list(&api, &ana).await.links[0].views, 5);
}
