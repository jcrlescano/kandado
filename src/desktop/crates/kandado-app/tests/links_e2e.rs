//! Phase 6's exit criteria for the desktop app, end to end, against the real
//! `kandado-server` and PostgreSQL:
//!
//! - a secure link opens for someone who is not signed in, and shows the item;
//! - it stops after its view limit, and when it is revoked.
//!
//! That the server never receives the key follows from the request type:
//! `CreateLink` has no field that could carry it, and the key is only ever
//! put after the `#` of the address the server answers with.
//!
//! Expiry is covered by the server's own tests (`http/link_tests.rs`), which
//! can move a link's dates back.
//!
//! ```text
//! docker compose -f src/backend/docker-compose.yml up -d
//! cargo build -p kandado-server
//! DATABASE_URL=postgres://kandado:kandado@127.0.0.1:54329/kandado \
//!   cargo test -p kandado-app --test links_e2e -- --ignored
//! ```

mod common;

use std::sync::Arc;

use common::{Server, computer, google, start_server, unique_email};
use kandado_app::{KandadoApp, Phase, links::LinkOptions};
use kandado_model::{
    ItemContent, ItemData,
    item::{Login, Passkey},
    link::{self, FieldStyle},
};
use kandado_protocol::{LinkState, OpenedLink};

const PASSWORD: &str = "correct horse battery staple";

async fn signed_in(server: &Server) -> Arc<KandadoApp> {
    let app = computer("links", server);
    app.restore().await.unwrap();
    google(&app, server, &unique_email()).await.unwrap();
    app.create_master_password(PASSWORD.into()).await.unwrap();
    assert!(matches!(
        app.confirm_recovery_saved().await.unwrap(),
        Phase::Unlocked { .. }
    ));
    app
}

fn github() -> ItemContent {
    ItemContent::new(
        "GitHub",
        ItemData::Login(Login {
            username: "juan".into(),
            password: "hunter2-but-longer".into(),
            totp: Some("otpauth://totp/GitHub?secret=JBSWY3DPEHPK3PXP".into()),
            ..Login::default()
        }),
    )
}

/// What someone with the link and nothing else gets: the payload from the
/// public route, opened with the key from the fragment.
async fn open_as_stranger(url: &str) -> Result<(link::LinkSnapshot, OpenedLink), u16> {
    let (address, fragment) = url.split_once('#').expect("the key is in the fragment");
    let id = address.rsplit('/').next().unwrap();
    let base = address.split("/l/").next().unwrap();
    let reply = reqwest::Client::new()
        .post(format!("{base}/v1/public/links/{id}/open"))
        .send()
        .await
        .unwrap();
    if !reply.status().is_success() {
        return Err(reply.status().as_u16());
    }
    let opened: OpenedLink = reply.json().await.unwrap();
    let key = link::key_from_fragment(fragment).unwrap();
    let id = uuid::Uuid::parse_str(id).unwrap();
    let snapshot = link::open(&key, *id.as_bytes(), opened.payload_enc.as_bytes()).unwrap();
    Ok((snapshot, opened))
}

#[tokio::test]
#[ignore = "needs PostgreSQL and a built kandado-server"]
async fn a_link_opens_for_a_stranger_until_its_views_run_out() {
    let server = start_server().await;
    let app = signed_in(&server).await;
    let vault = app.vaults().unwrap()[0].id;
    // Made offline-first: the server has not seen this item when the link is asked for.
    let item = app.create_item(vault, &github()).unwrap();
    assert!(app.can_share(item));

    let made = app
        .create_link(
            item,
            LinkOptions {
                expiry: 1,
                views: 1, // 5 times
                include_totp: true,
            },
        )
        .await
        .unwrap();
    assert!(
        made.url
            .starts_with(&format!("{}/l/{}#", server.url, made.id))
    );
    assert_eq!(made.max_views, Some(5));

    // Four strangers, then the fifth takes the last view.
    for left in (0..5).rev() {
        let (snapshot, opened) = open_as_stranger(&made.url).await.unwrap();
        assert_eq!(opened.views_left, Some(left));
        assert_eq!(snapshot.title, "GitHub");
        let password = snapshot
            .fields
            .iter()
            .find(|field| field.label == "Password")
            .unwrap();
        assert_eq!(password.style, FieldStyle::Secret);
        assert_eq!(password.value.expose(), "hunter2-but-longer");
        assert!(
            snapshot
                .fields
                .iter()
                .any(|field| field.style == FieldStyle::Totp)
        );
    }
    assert_eq!(open_as_stranger(&made.url).await.unwrap_err(), 404);

    let rows = app.links().await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title.as_deref(), Some("GitHub"));
    assert_eq!(rows[0].info.state, LinkState::UsedUp);
    assert_eq!(rows[0].info.views, 5);
}

#[tokio::test]
#[ignore = "needs PostgreSQL and a built kandado-server"]
async fn a_revoked_link_stops_at_once() {
    let server = start_server().await;
    let app = signed_in(&server).await;
    let vault = app.vaults().unwrap()[0].id;
    let item = app.create_item(vault, &github()).unwrap();
    app.sync_now().await.unwrap();

    let made = app.create_link(item, LinkOptions::default()).await.unwrap();
    let (_, fragment) = made.url.split_once('#').unwrap();

    // Without the two-factor option, the setup key stays home.
    let (snapshot, opened) = open_as_stranger(&made.url).await.unwrap();
    assert_eq!(opened.views_left, Some(0));
    assert!(
        snapshot
            .fields
            .iter()
            .all(|field| field.style != FieldStyle::Totp)
    );

    let second = app
        .create_link(
            item,
            LinkOptions {
                views: 3,
                ..LinkOptions::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(second.max_views, None);
    assert!(open_as_stranger(&second.url).await.is_ok());
    app.revoke_link(second.id).await.unwrap();
    assert_eq!(open_as_stranger(&second.url).await.unwrap_err(), 404);

    // What the server sent back holds no key: the fragment is added here.
    let listed = app.links().await.unwrap();
    assert!(
        listed
            .iter()
            .all(|row| row.info.id != made.id || row.info.views == 1)
    );
    assert_eq!(fragment.len(), 43);

    let states: Vec<_> = app
        .links()
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.info.state)
        .collect();
    assert_eq!(states, [LinkState::Revoked, LinkState::UsedUp]);
}

#[tokio::test]
#[ignore = "needs PostgreSQL and a built kandado-server"]
async fn a_passkey_cannot_be_linked() {
    let server = start_server().await;
    let app = signed_in(&server).await;
    let vault = app.vaults().unwrap()[0].id;
    let item = app
        .create_item(
            vault,
            &ItemContent::new("Site", ItemData::Passkey(Passkey::default())),
        )
        .unwrap();
    assert!(!app.can_share(item));
    let error = app
        .create_link(item, LinkOptions::default())
        .await
        .unwrap_err();
    assert!(matches!(
        error,
        kandado_app::AppError::Model(kandado_model::Error::NotShareable)
    ));
    assert!(app.links().await.unwrap().is_empty());
}
